# Linux Setup Guide

## Prerequisites

- Linux with GNOME Shell 49+ and Wayland
- Either Nix (what `install.sh` uses) or a Rust toolchain, CMake, and a C++
  compiler for building by hand — the local speech-to-text engine compiles
  [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) natively
- `pw-record` (PipeWire) for audio capture
- `ffmpeg` for audio encode/decode
- `wl-copy` (wl-clipboard) for clipboard support
- `pw-play` (PipeWire) for completion sound
- One transcription backend:
  - **local** (default recommendation): a Parakeet GGUF model, fully offline
    (downloaded automatically on first run)
  - **groq** (cloud): a Groq API key from https://console.groq.com/keys

## Quick Start

### 1. Build (native)

```bash
cd linux
cargo build --release
```

The build offloads inference to the GPU through ggml's Vulkan backend by
default — the default model streams buffered and needs it to keep up (see
[Swapping the model](#swapping-the-model)). On NixOS that needs the Vulkan
headers and shader compiler:

```bash
nix-shell -p cargo rustc cmake gcc pkg-config python3 vulkan-headers vulkan-loader shaderc \
  --run 'cargo build --release'
```

The backend is chosen at runtime: the GPU is used when one is present and the
CPU when it is not, so a Vulkan build still runs on a GPU-less machine. For a
CPU-only binary use `cargo build --release --no-default-features` (that needs
only `cargo rustc cmake gcc pkg-config`).

### 2. Cross-compile for aarch64 (e.g., MNT Pocket Reform)

Requires QEMU aarch64 emulation and Nix with flake support:

```bash
cd linux
nix-build default-cross.nix
```

The resulting binary is at `result/bin/onescribe-linux-core`. Copy it and the
`assets/` directory to the target machine (e.g., `~/bin/`).

See [default-cross.nix](default-cross.nix) for details on asset bundling and
ELF interpreter patching.

The cross build is CPU-only, so pair it with the cache-aware model rather than
the default (which streams buffered and needs the GPU):

```json
{"model_path":"~/.local/share/onescribe/models/nemotron-speech-streaming-en-0.6b-Q8_0.gguf"}
```

### 3. Transcription backend

#### Default: local (offline, no configuration needed)

Transcription runs on-device via
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) with the
Parakeet Unified English 0.6B model (~697 MB, Q8_0 quant).
Audio is transcribed **live while you speak** — the transcript is ready
almost instantly when you stop recording, regardless of how long you spoke.

**No setup is required**: local is the default backend, and the model is
downloaded automatically on first run (one-time, ~697 MB, resumable if
interrupted) to `~/.local/share/onescribe/models/`. To download it
manually instead:

```bash
mkdir -p ~/.local/share/onescribe/models
curl -L -o ~/.local/share/onescribe/models/parakeet-unified-en-0.6b-Q8_0.gguf \
  'https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/main/parakeet-unified-en-0.6b-Q8_0.gguf'
```

Optional keys in `~/.config/onescribe/config.json`: any GGUF model
supported by transcribe.cpp works — point `"model_path"` at it (`~` is
expanded; models without streaming support, e.g. Parakeet TDT, fall back to
batch transcription after recording stops). The default model is English-only;
`"language"` is useful only when selecting a multilingual model:

```json
{"model_path":"~/.local/share/onescribe/models/parakeet-unified-en-0.6b-Q8_0.gguf"}
```

Environment overrides: `ONESCRIBE_BACKEND=local|groq`,
`ONESCRIBE_MODEL_PATH=/path/to/model.gguf`, and `ONESCRIBE_LANGUAGE`.

#### Swapping the model

The default is `parakeet-unified-en-0.6b`, the most accurate English model
transcribe.cpp ships that also streams: 1.60% WER on LibriSpeech test-clean and
3.99% on FLEURS-en, versus 2.31% / 6.43% for the Nemotron streaming model it
replaced (1.26% vs 2.45% on a 60-utterance test-clean sample).

It streams *buffered*, though — the encoder re-runs a sliding
`[left|chunk|right]` window for every chunk instead of reusing cache — so live
dictation costs roughly 6.5x the compute. That is affordable only on the GPU
(79.5 s of speech, 256 ms chunks, Q8_0, 12-core Ryzen AI 9 HX PRO 375 +
Radeon 890M):

| Model | CPU | Vulkan |
| --- | ---: | ---: |
| `parakeet-unified-en-0.6b` (default) | 0.74x realtime — falls behind, text lags | 12.79x realtime — keeps up comfortably |
| `nemotron-speech-streaming-en-0.6b` | 4.97x realtime | 17.29x realtime |

Lower-latency presets make this worse, not better: the left context is pinned
at 5.6 s, so `(70,7,7)` re-encodes that same window more often and costs 12.0x
per audio-second versus 7.4x at the `(70,13,13)` default.

So on a GPU-less machine — or a `--no-default-features` build — switch to
Nemotron, whose cache-aware encoder keeps up on CPU:

```json
{"model_path":"~/.local/share/onescribe/models/nemotron-speech-streaming-en-0.6b-Q8_0.gguf"}
```

#### Alternative: Groq (cloud)

Used only when explicitly selected with `"backend":"groq"`:

```bash
mkdir -p ~/.config/onescribe
cat > ~/.config/onescribe/config.json <<'JSON'
{"backend":"groq","groq_api_key":"your-groq-api-key"}
JSON
```

(The key can also come from the `GROQ_API_KEY` environment variable, but the
`"backend":"groq"` selection is required either way — an API key alone no
longer switches the backend.)

### 4. Run the daemon

```bash
./install.sh
```

That builds the Nix package, installs it to `~/.local/opt/onescribe` (override
with `ONESCRIBE_PREFIX`), and starts it as the systemd user service
`onescribe.service`. It writes the unit only when you have none — pass
`--force-unit` to rewrite an existing one. Re-run the same command after pulling
to rebuild and restart. The GGUF model is not bundled: the first start downloads
it (~697 MB, resumable).

Installing rather than running `target/release/...` matters — the packaged
binary carries its own assets, so it survives `cargo clean` and rebuilds.

By hand the build is `nix-build default.nix --out-link ~/.local/opt/onescribe`.
It uses the Vulkan backend by default; `--arg vulkan false` gives a CPU-only
build (`--argstr` would pass the *string*
`"false"`, which Nix treats as truthy). To run it as a user service:

```ini
# ~/.config/systemd/user/onescribe.service
[Unit]
Description=OneScribe Linux transcription daemon
After=network.target pipewire.service wireplumber.service

[Service]
Type=simple
ExecStart=%h/.local/opt/onescribe/bin/onescribe-linux-core daemon
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
```

then `systemctl --user enable --now onescribe.service`.

> Running a bare `cargo build --release` binary as the daemon does not work:
> release builds resolve assets relative to the binary
> (`platform.rs::bundled_asset_path`) and `target/release/` has none next to it.
> Debug builds resolve them through `CARGO_MANIFEST_DIR`, which is why
> `cargo run` appears to work and hides this.

This starts:
- A system tray icon (via StatusNotifierItem/KDE sysext)
- A GlobalShortcuts portal session bound to **Alt+Space**
- A GNOME custom keyboard shortcut for **Alt+Space** (registered automatically
  via `gsettings` on every startup — this is the reliable hotkey path on
  GNOME 50)


## Usage

- Press **Alt+Space** to start recording.
- Press **Alt+Space** again to stop recording and transcribe.
- The transcribed text is copied to the clipboard. Paste it manually with `Ctrl+V` or `Ctrl+Y` in Emacs.
- Click the tray icon to toggle recording or quit.

## Other Commands

```bash
onescribe-linux-core sources             # list audio sources
onescribe-linux-core record [seconds]    # record and transcribe (one-shot)
onescribe-linux-core <audio-file>        # transcribe an existing file
```


## Key File Locations

| File | Purpose |
|------|---------|
| `~/.config/onescribe/config.json` | Backend, model path, and API key configuration |
| `~/.local/share/onescribe/models/` | Local GGUF speech-to-text models |
| `~/.local/share/applications/com.onescribe.OneScribe.desktop` | App registration (auto-generated by daemon) |
| `~/.config/autostart/onescribe.desktop` | Autostart entry |
| `~/.local/opt/onescribe/bin/onescribe-linux-core` | Installed binary (`ONESCRIBE_PREFIX` to change) |
| `~/.local/opt/onescribe/bin/assets/` | Bundled assets (icons, sounds) |
| `~/.config/systemd/user/onescribe.service` | User service (`./install.sh` writes it) |
| `~/Documents/OneScribe/` | Transcription journal (Markdown) |

## Architecture

The daemon (`onescribe-linux-core daemon`) runs two components:

1. **Hotkey listener** (`hotkey_daemon.rs`): Attempts to register Alt+Space via
   the `org.freedesktop.portal.GlobalShortcuts` portal (`ashpd`). On GNOME 50
   this reliably fails, so on every startup the daemon also registers Alt+Space
   as a GNOME custom keyboard shortcut via `gsettings`, which writes to a
   control FIFO the daemon listens on. Either path toggles recording when
   triggered.

2. **System tray icon** (`tray_app.rs`): Uses `ksni` (StatusNotifierItem) to
   provide a tray icon with status indication and manual toggle/quit controls.

Both are coordinated via `tokio` async channels.
