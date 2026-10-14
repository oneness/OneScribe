# OneScribe Speed and Accuracy Review

**Review date:** February 7, 2026  
**Focus:** Improve end-to-end latency first, then accuracy without large speed regressions.

## Summary

Current model selection (`whisper-large-v3-turbo`) is already one of the fastest speech-to-text options on Groq. Most additional wins in this repo are from removing fixed waits, moving work off the main actor, and tightening request/configuration knobs (`language`, `prompt`, model switch).

## Findings (Prioritized)

### Critical

1. Hardcoded API key in source
- File: `OneScribe/Managers/GroqTranscriber.swift:80`
- Risk: security incident, key abuse, rate limiting, and unstable latency/cost.
- Action: rotate key immediately and load from Keychain or environment.

### High

1. Fixed 500ms wait on every recording stop
- File: `OneScribe/Managers/RecordingManager.swift:111`
- Impact: guaranteed +0.5s latency per transcription.
- Action: trigger completion from recorder delegate (`audioRecorderDidFinishRecording`) instead of `asyncAfter`.

2. Transcription flow pinned to main actor
- File: `OneScribe/App/OneScribeApp.swift:116`
- Impact: network and follow-up work can block UI responsiveness and add scheduling delays.
- Action: run transcribe/journal work off-main, switch to `MainActor.run` only for UI state updates.

3. Blocking sleep in paste flow
- File: `OneScribe/Managers/AutoPaster.swift:37`
- Impact: unnecessary 100ms stall, can freeze caller thread.
- Action: remove `Thread.sleep`; if needed, use non-blocking coordination.

4. Missing speech accuracy controls in request
- File: `OneScribe/Managers/GroqTranscriber.swift:42-46`
- Impact: no explicit `language` and no vocabulary/context `prompt`, leaving accuracy on the table.
- Action: add optional multipart fields for `language` and `prompt`.

5. No compilable test safety net
- File: `OneScribeTests/OneScribeTests.swift:455`, `OneScribeTests/OneScribeTests.swift:659`, `OneScribeTests/OneScribeTests.swift:682`, `OneScribeTests/OneScribeTests.swift:334`
- Impact: optimization work can regress behavior with no reliable guard.
- Action: fix stale test APIs and protocol mismatches before refactors.

### Medium

1. Whole-file load before upload
- File: `OneScribe/Managers/GroqTranscriber.swift:27`
- Impact: memory spikes and poorer scaling with long audio files.
- Action: stream multipart body or enforce tighter recording length limits.

2. Temp recordings are not cleaned up
- File: `OneScribe/Managers/RecordingManager.swift:78-80`
- Impact: temp directory growth over time.
- Action: delete file after transcription success/failure.

3. Journal write path rewrites full file each save
- File: `OneScribe/Managers/TranscriptionJournal.swift:61-79`
- Impact: unnecessary I/O as history grows.
- Action: append-only writing via file handle.

4. Excessive debug logging in hotkey path
- File: `OneScribe/Hotkeys/HotkeyManager.swift:96`, `OneScribe/Hotkeys/HotkeyManager.swift:107`
- Impact: avoidable overhead and noisy logs during key monitoring.
- Action: gate debug logging by build config/log level.

5. Coarse HTTP error mapping
- File: `OneScribe/Managers/HTTPClient.swift:27-32`
- Impact: harder to tune retries/backoff for rate limits and transient failures.
- Action: parse response body/status into structured errors (429/5xx/timeouts).

## Fastest Model Guidance (Groq)

As of **February 7, 2026**:

1. `whisper-large-v3-turbo`
- Best default for speed in your current integration.
- Already used in `OneScribe/Managers/GroqTranscriber.swift:45`.

2. `distil-whisper-large-v3-en` (English-only)
- Potentially faster than turbo on Groq model card.
- Tradeoff: lower multilingual support and possible accuracy loss depending on your data.

3. `whisper-large-v3`
- Slower than turbo, but typically better accuracy/WER.

## Recommended Implementation Order

1. Security and config
- Remove hardcoded API key.
- Add runtime config for model selection (`turbo`, `v3`, optional distil).

2. Latency quick wins
- Remove `asyncAfter(0.5)` and `Thread.sleep(0.1)`.
- Move network and file I/O off main actor.

3. Accuracy knobs
- Add optional `language` and `prompt` parameters to transcription request.
- Provide a simple settings UI or config for these values.

4. Reliability and scaling
- Add temp file cleanup.
- Change journal to append-only writes.
- Improve HTTP error classification and retry/backoff.

5. Validation
- Fix test suite compile errors.
- Add basic latency instrumentation: record `record_stop -> paste_complete` and `upload_start -> transcript_received`.

## Suggested Performance Metrics

Track these at minimum:

- `T1`: record stop to upload start
- `T2`: upload start to transcript received
- `T3`: transcript received to paste complete
- `T_total`: record stop to paste complete

Targets for short dictation (example):
- `T1 < 150ms`
- `T2 < 1200ms` (turbo, good network)
- `T3 < 100ms`
- `T_total < 1.5s`

## External References

- Groq speech-to-text docs: https://console.groq.com/docs/speech-to-text
- Groq `whisper-large-v3-turbo` model page: https://console.groq.com/docs/model/whisper-large-v3-turbo
- Groq `whisper-large-v3` model page: https://console.groq.com/docs/model/whisper-large-v3
- Groq `distil-whisper-large-v3-en` model page: https://console.groq.com/docs/model/distil-whisper-large-v3-en
