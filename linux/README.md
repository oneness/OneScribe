# OneScribe on Linux

Press `Alt+Space` and speak — the transcript lands on your clipboard.

Transcription runs **on-device by default**: no API key, no account, and the
audio never leaves the machine. The model downloads itself on first run and
transcribes live while you speak.

Needs GNOME Shell 49+ on Wayland, PipeWire (`pw-record`, `pw-play`), `ffmpeg`,
`wl-clipboard`, and Nix for the installer.

**Curious?**
[Architecture](../docs/architecture.md) ·
[Setup details](../docs/linux.md)

**Want to try?**
```bash
./install.sh
```

That builds the package, installs it to `~/.local/opt/onescribe`, and runs it as
the systemd user service `onescribe.service`. Re-run it after pulling to rebuild
and restart. The model (~697 MB) is downloaded on first start.

## Use

- `Alt+Space` starts recording; `Alt+Space` again stops and transcribes.
- The text is on your clipboard — paste it yourself.
- The tray icon toggles recording and quits.

## Configure

Nothing, by default. There are two optional one-liners for
`~/.config/onescribe/config.json`.

Groq cloud instead of on-device:

```json
{"backend":"groq","groq_api_key":"your-groq-api-key"}
```

A CPU-friendly model, on a machine without a GPU:

```json
{"model_path":"~/.local/share/onescribe/models/nemotron-speech-streaming-en-0.6b-Q8_0.gguf"}
```

Building by hand, cross-compiling for aarch64, model benchmarks, and file
locations: [docs/linux.md](../docs/linux.md).

## Contributing

Personal software. No pull requests.

## License

MIT — see [LICENSE](../LICENSE).
