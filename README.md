# OneScribe

Press a hotkey and speak to transcribe text on macOS or Linux. Linux copies the result to the clipboard; macOS also pastes it automatically.

On **Linux** it transcribes **on-device by default** — no API key, no account,
and the audio never leaves the machine. The model downloads itself on first run
and transcription streams live while you speak. On **macOS** it transcribes via
the Groq cloud API, which Linux can also be configured to use.

## Quick links

- **[Overview](https://www.birkey.co/OneScribe/)** — what it is and how it works (space-bar to navigate)
- **[Architecture](docs/architecture.md)** — internals for both platforms

## Quick Start

**On Linux**, nothing here is needed — see [linux/README.md](linux/README.md).
Build it, run the daemon, and the local model is fetched on first use.

### macOS

#### 1. Prerequisites
- macOS 14+
- Xcode 15+
- A Groq API key: https://console.groq.com/keys — macOS has no on-device
  backend, so this one is required

#### 2. Configure your Groq API key
Use one of these methods.

Option A (recommended): interactive setup (works for Finder/Xcode/Terminal launches)
```bash
make setup
```

Option B: environment variable (best when launching from Terminal)
```bash
export GROQ_API_KEY="your-groq-api-key"
```

Option C: config file (works for Finder/Xcode launches too)
```bash
mkdir -p ~/Library/Application\ Support/OneScribe
cat > ~/Library/Application\ Support/OneScribe/config.json <<'JSON'
{"groq_api_key":"your-groq-api-key"}
JSON
```

Optional override for custom config location:
```bash
export ONESCRIBE_CONFIG_PATH="/absolute/path/to/config.json"
```

#### 3. Build and run
```bash
make compile
make run
```

Or do a full local validation (clean + build + test + codesign + open Accessibility settings):
```bash
make build
```

## Required macOS Permissions

### Accessibility (required for global hotkey + auto-paste)
1. Open `System Settings`.
2. Go to `Privacy & Security` -> `Accessibility`.
3. Enable `OneScribe`.
4. If `OneScribe` is not listed, click `+` and add the built app (`OneScribe.app`), then enable it.

Shortcut command to open the Accessibility panel directly:
```bash
make access
```

### Microphone (required for recording)
1. Open `System Settings`.
2. Go to `Privacy & Security` -> `Microphone`.
3. Enable `OneScribe`.

## Usage
- Press `Option + Space` to start recording.
- Press `Option + Space` again to stop recording and transcribe.
- On macOS, the transcribed text is copied to the clipboard and pasted automatically.
- On Linux, it is copied to the clipboard for you to paste manually.
- Click the menu bar icon to start/stop recording or quit.

## Common Commands
```bash
make help      # list targets
make setup     # prompt for API key and save config
make compile   # build app
make test      # run tests
make run       # open built app
make access    # open Accessibility settings
```

## Linux

See [linux/README.md](linux/README.md) for the full setup guide.

No config file is needed: on-device transcription via transcribe.cpp is the
default, and the model downloads automatically on first run.

Config, if you want it, uses XDG resolution:

- Default config file: `~/.config/onescribe/config.json`
- Override: `ONESCRIBE_CONFIG_PATH=/absolute/path/to/config.json`

To use the Groq cloud backend instead of the local one:

```bash
mkdir -p ~/.config/onescribe
cat > ~/.config/onescribe/config.json <<'JSON'
{"backend":"groq","groq_api_key":"your-groq-api-key"}
JSON
```

## Contributing

This project is personal software that I maintain for my own use. I do not accept pull requests.

If it's useful to you: fork it, copy the code, adapt it freely. The only ask is that you keep the copyright notice intact (MIT license).

## License

MIT — see [LICENSE](LICENSE).
