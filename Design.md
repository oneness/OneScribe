# OneScribe Design

## Overview

OneScribe is a macOS menu bar app for fast dictation:
1. Record from the microphone.
2. Transcribe with Groq Whisper.
3. Auto-paste the transcript into the active app.
4. Save transcripts to a daily Markdown journal.

## Current User Flow

1. User presses `Option + Space` to start recording.
2. User presses `Option + Space` again to stop.
3. App uploads audio to Groq (`whisper-large-v3-turbo`).
4. App copies transcript to clipboard and simulates `Cmd+V`.
5. App appends transcript to `~/Documents/OneScribe/YYYY-MM-DD.md`.

## Hotkey

| Hotkey | Action |
|--------|--------|
| `Option + Space` | Toggle recording start/stop |

## App States

| State | Meaning |
|-------|---------|
| `idle` | Ready to record |
| `recording` | Capturing audio |
| `transcribing` | Uploading/transcribing audio |

Status icon is updated by `StatusItemController` based on `RecordingManager.state`.

## Key Components

- `OneScribe/App/AppDelegate.swift`
- `OneScribe/Managers/RecordingManager.swift`
- `OneScribe/Managers/GroqTranscriber.swift`
- `OneScribe/Managers/AutoPaster.swift`
- `OneScribe/Managers/TranscriptionJournal.swift`
- `OneScribe/Hotkeys/HotkeyManager.swift`
- `OneScribe/UI/StatusItemController.swift`

## Data and Config

- API key resolution order:
1. `GROQ_API_KEY` env var
2. `~/Library/Application Support/OneScribe/config.json`
3. `ONESCRIBE_CONFIG_PATH` override (optional)

- Journal location:
1. `~/Documents/OneScribe/`
2. One Markdown file per day

## Required Permissions

- `Accessibility`: required for global hotkey monitoring and synthetic key events for auto-paste.
- `Microphone`: required for audio capture.
- `Network`: required to call Groq transcription API.

## Build and Run

```bash
make compile
make run
```

Optional full validation:

```bash
make build
```
