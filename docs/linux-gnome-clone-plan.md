# OneScribe Linux Clone Plan

Last updated: March 7, 2026

## Goal

Build a Linux clone of the current macOS OneScribe app with GNOME on NixOS as the primary target.

Target behavior:

1. Record microphone audio with a global hotkey.
2. Stop recording with the same hotkey.
3. Send audio to Groq Whisper.
4. Copy the transcript to the clipboard for manual pasting.
5. Append the transcript to a daily Markdown journal.
6. Expose a small desktop presence similar to the macOS menu bar app.

## Current macOS Behavior To Preserve

The existing app is small and mostly organized around one workflow:

1. `Option + Space` toggles recording.
2. `idle -> recording -> transcribing -> idle`
3. Groq endpoint: `/openai/v1/audio/transcriptions`
4. Model: `whisper-large-v3-turbo`
5. Journal file: `YYYY-MM-DD.md`
6. Journal entry format:

```md
# Transcriptions - {Long Date}

## {h:mm a}

{transcribed text}

---
```

Config behavior to preserve:

1. `GROQ_API_KEY` environment variable wins.
2. `ONESCRIBE_CONFIG_PATH` may point to a JSON config file.
3. Config file accepts `groq_api_key` or `GROQ_API_KEY`.

## Linux Reality Check

An exact clone is plausible for recording, transcription, journaling, clipboard, packaging, and startup.

The risky parts are GNOME/Wayland desktop integration:

1. Global shortcuts are feasible through the XDG desktop portal.
2. Arbitrary synthetic key injection is restricted on Wayland.
3. Tray or status icon support is environment-dependent on GNOME.

For reliability, the Linux implementation intentionally does not inject paste keystrokes. It copies each transcript to the clipboard and lets the user paste it with the target application's normal shortcut.

## Language Decision

## Recommendation: Rust

Rust is the best fit for this clone.

Why:

1. Strong fit for a daemon plus desktop-integration architecture.
2. Good async/networking support for Groq calls.
3. Good D-Bus and portal integration options.
4. Strong testability for a TDD-first approach.
5. Good Nix and NixOS packaging story.
6. Easy to keep the core logic platform-neutral and heavily unit-tested.

## Alternatives

### Zig

Good for a low-level daemon. Weak compared with Rust for desktop ecosystem leverage, tests, and long-term maintainability for this app shape.

### Go

Reasonable fallback if delivery speed matters more than strict control. Less attractive for desktop UX and lower-level Linux integration than Rust.

### Hare

Interesting language, but too immature for GNOME desktop integration, packaging, and long-lived maintenance here.

### Janet

Good embedded scripting language, poor choice for a desktop application whose hardest problems are native integration and distribution.

## Proposed Architecture

Split the Linux version into clear layers.

### 1. Core crate

Pure Rust, no GNOME bindings.

Responsibilities:

1. App state machine.
2. Config resolution.
3. Groq request construction and response parsing.
4. Journal formatting and append rules.
5. History persistence rules.
6. Orchestration logic expressed via traits.

### 2. Platform adapter crate

Linux- and GNOME-facing integrations behind traits.

Responsibilities:

1. Microphone recording.
2. Global shortcut registration.
3. Clipboard access.
4. Clipboard completion behavior.
5. Notifications.
6. Config and journal path discovery using XDG rules.

### 3. Desktop shell

Thin executable that wires the core to real adapters.

Responsibilities:

1. Lifecycle.
2. Status icon or equivalent desktop presence.
3. Settings surface if needed.
4. Startup activation.

## Linux-Specific Design Choices

### Config paths

Use Linux-native defaults:

1. `GROQ_API_KEY` environment variable.
2. `ONESCRIBE_CONFIG_PATH` override.
3. `${XDG_CONFIG_HOME:-~/.config}/onescribe/config.json`

### Journal path

Use:

1. `${XDG_DOCUMENTS_DIR:-~/Documents}/OneScribe`

Keep the markdown format byte-for-byte compatible with the macOS app.

### Hotkey

Use the XDG Global Shortcuts portal first.

Target default:

1. `Alt + Space`

This matches the macOS mental model, but we should validate that GNOME does not conflict with it on the target setup.

### Clipboard

Copy completed transcripts to the system clipboard. Do not attempt direct key injection on Wayland.

## TDD Plan

Implementation order should follow risk and testability.

### Phase 1: Pure core

Write tests first for:

1. State transitions.
2. Config resolution precedence.
3. Config JSON parsing.
4. Journal filename and entry formatting.
5. Groq request body construction.
6. Error mapping from API responses.

### Phase 2: Orchestration

Write tests first for:

1. `start -> stop -> transcribe -> copy -> journal`
2. Failure paths for recording, network, auth, empty transcript, and clipboard access.
3. Temp-file cleanup behavior.

### Phase 3: Linux adapters

Write contract tests and local integration tests for:

1. Recorder adapter.
2. Portal/global-shortcut adapter.
3. Clipboard adapter.
4. Notification adapter.

### Phase 4: GNOME desktop shell

Write manual acceptance checks for:

1. Tray/status presence.
2. Startup activation.
3. Portal permission prompts.
4. Wayland clipboard behavior.

## First Deliverable

The first implementation slice should be intentionally small:

1. Rust core crate.
2. Tests for config, state machine, and journal format.
3. Simple CLI proof:
   - accepts an audio file path,
   - transcribes it,
   - writes the journal entry,
   - copies to clipboard.

This gives us a real, testable product core before we spend time on GNOME shell mechanics.
