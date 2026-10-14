# OneScribe iOS POC Design

Last updated: 2026-02-12

## 0. Finalized Decisions

1. Both iOS and macOS should prefer a shared iCloud journal path, with platform-local fallback.
2. Auto-start recording only on cold launch and Action Button launch.
3. Foreground resume should not auto-start recording; it should show current transcript/history.
4. Local history is unlimited for now.
5. API key is entered via a simple in-app settings screen.

## 1. Objective

Build a very simple iOS version of OneScribe that:

1. Starts recording immediately when opened.
2. Stops recording with one tap (or action button shortcut trigger).
3. Transcribes audio with Groq Whisper.
4. Saves the transcript to the same daily markdown journal format as macOS.
5. Makes transcript immediately usable via Copy/Share.

Primary goal: speed and simplicity, not feature breadth.

## 2. Product Requirements (POC)

### Must Have

1. Single-screen app with 3 states: `recording`, `transcribing`, `ready`.
2. Auto-start recording on launch.
3. One primary button toggles `Start/Stop` (or `Stop` while active).
4. After transcription:
   - show text,
   - allow `Copy`,
   - allow `Share`.
5. Save each transcription to `YYYY-MM-DD.md` and append new entries if file exists.
6. Show recent transcription items in a simple list; tap item to copy/share.
7. Support Action Button workflow via App Shortcut (`Toggle Recording`) where device supports it.

### Must Not Have (POC)

1. No global hotkeys.
2. No background always-on recording.
3. No automatic paste into other apps.
4. No complex editor, tags, folders, or account system.

## 3. iOS Constraints (Confirmed)

1. iOS app can copy text to clipboard.
2. iOS app cannot inject keystrokes into other apps for auto-paste.
3. Physical button integration is practical through Action Button + Shortcuts, with Back Tap as fallback.

Implication: UX is `record -> transcribe -> copy/share -> switch app -> paste`.

## 4. Journal Compatibility Requirement

iOS journal output must match macOS format exactly so files can be shared/merged safely.

Current macOS format:

```md
# Transcriptions - {Long Date}

## {h:mm a}

{transcribed text}

---
```

File naming rule:

1. One file per day: `YYYY-MM-DD.md`.
2. If file exists, append a new section.
3. If file does not exist, create it with header and first section.

## 5. Storage Strategy

Goal: both macOS and iOS append to the same logical journal with iCloud sync.

POC strategy:

1. iOS writes to iCloud Documents container:
   - `FileManager.default.url(forUbiquityContainerIdentifier: nil)?.appendingPathComponent("Documents/OneScribe")`
2. If iCloud container unavailable, fallback to local app Documents:
   - `.../Documents/OneScribe` inside app sandbox.
3. macOS app should also prefer the same iCloud location, then fallback to current `~/Documents/OneScribe`.

Rationale: this gives deterministic cross-device append behavior without extra backend.

## 6. UX Design (Simple/Clean)

Single screen:

1. Large status indicator (`Recording`, `Transcribing`, `Ready`).
2. One prominent primary button:
   - `Stop` while recording,
   - `Start` while idle.
3. Current transcript card with:
   - `Copy`,
   - `Share`.
4. Recent list (latest first), each row tap opens action sheet: `Copy` / `Share`.

Launch behavior:

1. On cold launch, if microphone permission already granted, auto-start recording.
2. On Action Button launch, trigger toggle flow (start/stop).
3. On normal foreground resume, do not auto-start; show current transcript and history.
4. If permission missing, show one permission CTA, then auto-start once granted for cold launch flow.

## 7. Technical Design

### Reuse from Existing Code

1. `GroqTranscriber` request/response flow (`OneScribe/Managers/GroqTranscriber.swift`).
2. `HTTPClient` abstraction (`OneScribe/Managers/HTTPClient.swift`).
3. `Transcription` model and history concepts.
4. Markdown journal append logic from `TranscriptionJournal` with platform-safe file APIs.

### Replace/Remove for iOS

1. Remove AppKit/Cocoa dependencies.
2. No `NSStatusBar`, no global key monitor, no CGEvent paste simulation.
3. Replace `NSPasteboard` with `UIPasteboard`.

### Suggested New Structure

1. `Shared/`
   - `TranscriptionService.swift`
   - `GroqTranscriber.swift`
   - `JournalWriter.swift` (platform-neutral file append logic)
   - `TranscriptionStore.swift` (recent list persistence)
2. `OneScribeiOS/`
   - `App/OneScribeiOSApp.swift`
   - `Features/Recorder/RecorderViewModel.swift`
   - `Features/Recorder/RecorderView.swift`
   - `Integrations/ClipboardService.swift`
   - `Integrations/ShareService.swift`
   - `Intents/ToggleRecordingIntent.swift`

## 8. State Machine (POC)

`idle -> recording -> transcribing -> ready`

Events:

1. `appLaunched`:
   - if mic authorized: `idle -> recording` automatically.
2. `stopTapped`:
   - `recording -> transcribing`.
3. `transcriptionSucceeded`:
   - `transcribing -> ready`.
4. `transcriptionFailed`:
   - `transcribing -> idle` with simple error banner.
5. `startTapped`:
   - `idle/ready -> recording`.

## 9. App Shortcut / Physical Button POC

Expose App Intent:

1. `ToggleRecordingIntent`.
2. If app not active, intent launches app and executes toggle.
3. User assigns this shortcut to Action Button in iOS settings.
4. Back Tap can run the same shortcut as fallback.

## 10. Error Handling (Minimal)

1. Microphone denied: show inline message + button to open Settings.
2. Network/API error: show inline retry action.
3. Empty transcription: show "No speech detected" and keep audio entry optional (POC: discard).

## 11. Security and Config

1. Store Groq API key in Keychain on iOS.
2. Simple settings screen for API key input.
3. No analytics for POC.

## 12. Implementation Plan

### Phase 1: Core iOS Skeleton

1. Add iOS target and scheme.
2. Build single-screen recorder UI with auto-start on launch.
3. Record audio and produce local `.m4a`.

### Phase 2: Transcription + Clipboard

1. Reuse/transplant Groq transcriber.
2. Wire stop -> transcribe -> ready flow.
3. Add Copy/Share actions.

### Phase 3: Journal Append + List

1. Implement markdown daily file writer with append semantics.
2. Save to iCloud Documents container with fallback.
3. Show recent items list with copy/share actions.
4. Keep local history unlimited for POC.

### Phase 4: Action Button Integration

1. Add App Intent and App Shortcut.
2. Validate double trigger flow: start then stop.
3. Document setup steps in README.

## 13. Acceptance Criteria

1. Cold launch to active recording in <= 2 taps (0 taps after permissions).
2. Stop recording yields transcript and `Copy` button in same session.
3. A second transcription on same day appends to same markdown file.
4. iOS markdown file format matches macOS file format.
5. Action Button can trigger start/stop through shortcut on supported devices.
6. Foreground resume does not start recording unless user taps Start or uses Action Button shortcut.
