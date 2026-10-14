# OneScribe architecture

## Overview

OneScribe is a global-hotkey dictation utility that runs as an invisible system tray / menu bar app. It has two independent platform implementations:

- **macOS** — Swift + AppKit, built with Xcode
- **Linux** — Rust, targets PipeWire + GNOME Wayland

Both implementations record, transcribe, copy the result to the clipboard, and append it to the journal. macOS then auto-pastes the result; Linux intentionally leaves it on the clipboard for manual pasting. macOS transcribes via the Groq cloud API; Linux transcribes locally on-device by default (transcribe.cpp + Nemotron), with Groq as a config-selectable alternative.

---

## The pipeline

```
hotkey press
    │
    ▼
audio capture (AVAudioRecorder / pw-record)
    │
    ▼
transcription
  · Linux local:  transcribe.cpp (ggml) + Nemotron Speech Streaming English 0.6B GGUF,
                  offline, streamed live while recording
  · cloud:        Groq Whisper API (whisper-large-v3-turbo) — macOS, or Linux with backend=groq
    │
    ▼
clipboard  (NSPasteboard / wl-copy, xclip, or xsel)
    │
    ├── macOS: synthesise Cmd+V
    └── Linux: manual paste
    │
    ▼
journal append  (~/Documents/OneScribe/YYYY-MM-DD.md)
```

Second hotkey press stops recording and triggers the rest of the pipeline. The app never opens a window during normal use.

---

## macOS implementation (Swift)

### Entry point

`OneScribe/main.swift` creates `NSApplication` and sets `AppDelegate` as delegate.
`OneScribe/App/OneScribeApp.swift` (`AppDelegate`) wires everything up in `applicationDidFinishLaunching`:

1. Creates `StatusItemController` (menu bar icon)
2. Calls `HotkeyManager.shared.setup()`
3. Calls `setupRecordingWorkflow()` — observes `RecordingManager.shared.$state` via Combine and sets `onRecordingComplete` callback

### Key components

| File | Role |
|------|------|
| `UI/StatusItemController.swift` | Menu bar icon — draws a microphone glyph via `NSBezierPath`, shows state badges (red dot = recording, grey pill = transcribing) |
| `Hotkeys/HotkeyManager.swift` | `NSEvent.addGlobalMonitorForEvents(.keyDown)` — detects `Option+Space`, calls `RecordingManager.shared.toggle()` |
| `Managers/RecordingManager.swift` | `AVAudioRecorder` → `.m4a` at 16 kHz mono AAC; fires `onRecordingComplete(url)` after a 500 ms settle delay |
| `Managers/GroqTranscriber.swift` | Multipart POST to Groq API via `URLSession`; 30-second timeout via `withThrowingTaskGroup` |
| `Managers/AutoPaster.swift` | Copies to `NSPasteboard`, then synthesises `Cmd+V` via `CGEvent` on `cghidEventTap` |
| `Managers/TranscriptionJournal.swift` | Appends to `~/Documents/OneScribe/YYYY-MM-DD.md` with `## HH:MM` headers |
| `Managers/HTTPClient.swift` | `URLSession` wrapper; maps 401 → `authenticationFailed`, other non-200 → `networkError` |
| `Models/AppState.swift` | `enum AppState { idle, recording, transcribing }` |
| `Models/TranscriptionError.swift` | Typed errors: `networkError`, `timeout`, `authenticationFailed`, `fileNotFound`, … |

### Hotkey

Uses `NSEvent.addGlobalMonitorForEvents(matching: .keyDown)` — requires Accessibility permission. Intercepts `Option+Space` (keyCode 49 + `.option` modifier). `Cmd+V` is deliberately not intercepted to avoid an infinite loop with the auto-paster.

### Audio recording

`AVAudioRecorder` writes to `/tmp/recording_<timestamp>.m4a`. Settings: `kAudioFormatMPEG4AAC`, 16 kHz, mono, medium quality — tuned to Whisper's native sample rate to minimise upload size.

### Transcription

`GroqTranscriber` builds a `multipart/form-data` POST to `https://api.groq.com/openai/v1/audio/transcriptions` with model `whisper-large-v3-turbo`. The call races against a 30-second `Task.sleep` timeout so the app never hangs in `.transcribing` state.

### Auto-paste

`AutoPaster` copies the transcript to `NSPasteboard`, then synthesises `Cmd+V` using `CGEvent(keyboardEventSource:virtualKey:keyDown:)` posted via `.cghidEventTap`. Requires Accessibility permission.

### Dependency injection

`HotkeyManager` accepts `SystemEventMonitor` and `KeyCodeDetector` protocols; `GroqTranscriber` accepts `HTTPClient`. This allows full unit test coverage without system access.

---

## Linux implementation (Rust)

The crate at `linux/` is both a binary and a library (`lib.rs` re-exports everything for testability).

### Entry point

`linux/src/main.rs` dispatches four CLI modes:

- `onescribe <audio-file>` — transcribe an existing file
- `onescribe record [seconds]` — record for N seconds, then transcribe
- `onescribe daemon` — start the tray + hotkey daemon (normal use)
- `onescribe sources` — list PipeWire audio sources

### Module map

| Module | Role |
|--------|------|
| `state.rs` | `enum AppState { Idle, Recording, Transcribing }` |
| `config.rs` | Backend selection (`backend`: local/groq), model path, and API key resolution (env var → XDG config file); `resolve_config_path()` respects `XDG_CONFIG_HOME` and `ONESCRIBE_CONFIG_PATH` |
| `engine.rs` | `TranscriptionEngine` enum — local on-device inference via `transcribe-cpp` (ggml) with a Nemotron GGUF, or the Groq cloud path |
| `groq.rs` | Pure request builder and response parser — no I/O; validates 25 MB upload cap |
| `transport.rs` | `reqwest::blocking` HTTP execution; maps non-2xx to `TransportError` |
| `recorder.rs` | `pw-record` capture: live raw-PCM chunks over a channel (local daemon streaming) or piped into `ffmpeg` → Opus/OGG at 16 kbps (Groq + CLI); SIGINT to stop; `list_audio_sources()` via `wpctl` |
| `hotkey_daemon.rs` | Core async event loop: XDG GlobalShortcuts portal (+ auto-configured GNOME custom shortcut fallback) → `toggle_recording()` → full pipeline |
| `tray_app.rs` | `ksni` StatusNotifierItem tray; bridges events between tray and daemon via channels |
| `platform.rs` | Clipboard (`wl-copy` / `xclip` / `xsel`), sound (`pw-play`), journal write, icon install |
| `desktop.rs` | `probe_desktop_capabilities()` — checks GlobalShortcuts portal via `gdbus introspect` |
| `journal.rs` | Pure rendering: `render_journal_entry()` produces identical Markdown format to macOS |

### Hotkey

`ashpd` GlobalShortcuts portal (`org.freedesktop.portal.GlobalShortcuts`) — the modern sandboxed approach for GNOME Wayland. Binds `"toggle-recording"` with preferred trigger `Alt+Space`. Requires `xdg-desktop-portal-gnome`.

On GNOME 50, `gnome-control-center-global-shortcuts-provider` segfaults when `bind_shortcuts` tries to show its key-binding dialog without a parent window, so the portal bind reliably fails there. The daemon treats this as non-fatal and falls back to a GNOME custom keyboard shortcut (`<Alt>space` → writes `toggle` to a control FIFO at `/run/user/$UID/onescribe-control`), which it registers itself via `gsettings` on every startup (`ensure_gnome_custom_shortcut()` in `hotkey_daemon.rs`) — no separate setup script or manual dconf edit required. If GNOME ever fixes the crash, the portal path is used automatically without any code change.

### Audio recording

`pw-record` captures raw 16 kHz mono s16le PCM to stdout, consumed one of two ways:

- **`StreamingPwRecorder`** (local daemon) — a reader thread forwards sample-aligned chunks over a channel to the live transcription worker; no encode step, no temp file.
- **`PwRecordRecorder`** (Groq + CLI record mode) — piped into `ffmpeg` (Opus/OGG at 16 kbps, `voip` profile), written to `/tmp/onescribe-recording-<nanoseconds>.ogg`.

Stop sends SIGINT to `pw-record` via `libc::kill`, then drains the sink.

### Transcription

Selected at startup by `config.rs::resolve_backend()` into a `TranscriptionEngine` (`engine.rs`):

- **local (offline)** — `transcribe-cpp` (ggml) loads a Nemotron Speech Streaming English 0.6B GGUF once at startup (~0.3 s). Daemon recordings are transcribed **live**: `StreamingPwRecorder` forwards raw PCM chunks (256 ms) over a channel to a worker thread that feeds the model's streaming session, so the final transcript is ready ~instantly at stop (measured ~0.6 s stop-to-clipboard including journaling). Models without streaming support fall back to a batch run over the accumulated audio automatically. CLI file/record modes use batch: audio (file or recorder OGG) is decoded to PCM via `ffmpeg`. Model auto-resolves to `~/.local/share/onescribe/models/nemotron-speech-streaming-en-0.6b-Q8_0.gguf` and is **auto-downloaded on first run** (streamed to a `.partial` file, resumable via HTTP Range, size-verified, renamed into place; custom `model_path` values are never auto-downloaded). Overrides: `model_path`/`language` config keys, `ONESCRIBE_MODEL_PATH`, `ONESCRIBE_LANGUAGE`.
- **groq (cloud)** — same API and model as macOS. `build_groq_transcription_request()` in `groq.rs` is pure (no I/O) and fully unit-tested. `execute_http_request()` in `transport.rs` uses `reqwest::blocking` inside `tokio::task::spawn_blocking`.

### Clipboard

After a successful Linux transcription, the daemon copies the text with `wl-copy`, `xclip`, or `xsel`. It does not inject a paste shortcut; the user pastes the clipboard contents manually in the target application.

### Key dependencies

| Crate | Purpose |
|-------|---------|
| `ashpd 0.12` | XDG GlobalShortcuts portal bindings |
| `ksni 0.3` | StatusNotifierItem system tray |
| `transcribe-cpp 0.1` | Local speech-to-text inference (ggml; compiles the native library via CMake) |
| `reqwest 0.12` (blocking + rustls-tls) | HTTP client for Groq API |
| `tokio 1` | Async runtime for daemon event loop |
| `serde_json 1.0` | Config and API response parsing |
| `chrono 0.4` | Timestamps for journal entries |
| `libc 0.2` | SIGINT to `pw-record` child process |

---

## Configuration

Both platforms share the same JSON config file. On Linux, **local is the default backend** — no config file is needed at all for on-device transcription. Groq is used only when explicitly selected:

```json
{"backend": "groq", "groq_api_key": "your-key-here"}
```

(macOS only supports Groq.) `ONESCRIBE_BACKEND` overrides the config key.

API key resolution priority (identical on both platforms):

1. `GROQ_API_KEY` environment variable
2. Platform config file:
   - macOS: `~/Library/Application Support/OneScribe/config.json`
   - Linux: `~/.config/onescribe/config.json` (respects `XDG_CONFIG_HOME`)
3. `ONESCRIBE_CONFIG_PATH` env var overrides the config file path

---

## Journal format

Both platforms write identical Markdown:

```markdown
# Transcriptions - May 31, 2026

## 2:34 PM
The transcript text goes here.

---
```

Journal directory:

- macOS: `~/Documents/OneScribe/`
- Linux: `~/Documents/OneScribe/` (respects `XDG_DOCUMENTS_DIR`)

---

## Design decisions

**Zero UI.** The app is an invisible background utility. No window ever opens during normal use. The tray/menu bar icon and its badge state are the entire UI.

**Groq `whisper-large-v3-turbo`.** ~8× faster inference vs. `whisper-large-v3` with minimal accuracy loss. Audio is captured at 16 kHz mono (Whisper's native rate) to minimise upload size and latency.

**Audio format differs by platform.** macOS uses AAC/M4A (native AVFoundation, no external tools). Linux uses Opus/OGG via `pw-record | ffmpeg` (smaller files, but requires PipeWire and ffmpeg at runtime).

**Clipboard behavior differs by platform.** macOS synthesises `Cmd+V` through CoreGraphics after copying the transcript. Linux deliberately stops after copying to the clipboard, avoiding unreliable synthetic input on Wayland.

**Library split on Linux.** The Rust crate is both binary and library so all logic can be unit-tested without running the daemon. `groq.rs` and `journal.rs` are fully pure — no I/O, just data transformation — and are comprehensively tested.

---

## Source layout

```
OneScribe/                        macOS Swift app (Xcode)
  main.swift                      NSApplication entry point
  App/
    OneScribeApp.swift            AppDelegate — wires everything up
  UI/
    StatusItemController.swift    Menu bar icon and state badges
  Hotkeys/
    HotkeyManager.swift           Global Option+Space listener
  Managers/
    RecordingManager.swift        AVAudioRecorder wrapper
    GroqTranscriber.swift         Groq Whisper API client
    AutoPaster.swift              CGEvent Cmd+V synthesiser
    TranscriptionJournal.swift    Daily markdown journal
    HTTPClient.swift              URLSession wrapper protocol
    TranscriptionHistoryManager.swift  In-memory transcription history
  Models/
    AppState.swift                idle / recording / transcribing
    Transcription.swift           Codable transcript model
    TranscriptionError.swift      Typed error enum
  Resources/
    SetupChecker.swift            First-launch setup state
    WelcomeViewController.swift   Onboarding UI (scaffolded)

linux/                            Linux Rust crate
  src/
    main.rs                       CLI dispatcher (file/record/daemon/sources)
    lib.rs                        Public API re-exports
    state.rs                      AppState enum
    config.rs                     API key resolution, XDG paths
    groq.rs                       Pure Groq request builder + response parser
    transport.rs                  reqwest HTTP execution
    recorder.rs                   pw-record | ffmpeg pipeline
    hotkey_daemon.rs              Async event loop, toggle logic, pipeline
    tray_app.rs                   ksni tray, event bridging
    platform.rs                   Clipboard, sound, journal, icons
    desktop.rs                    Portal capability probing
    journal.rs                    Pure journal entry renderer

docs/
  index.html                      Executive presentation (GitHub Pages, space-bar to navigate)
  architecture.md                 This file
```
