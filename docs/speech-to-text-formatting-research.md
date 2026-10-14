# Speech-to-Text + Spoken Formatting Commands (V2)

**Last updated:** February 7, 2026  
**Purpose:** Validate whether users can speak formatting commands (for example, "make this professional") in the same audio recording and get formatted output in one pass.

## Executive Verdict

1. **With STT-only endpoints (Groq Whisper, OpenAI `audio/transcriptions`)**: not a guaranteed feature.
2. **With multimodal LLM audio input (OpenAI Chat Completions audio, Gemini audio understanding)**: feasible in one request, but command detection from speech is still probabilistic.
3. **For production reliability**: use a deterministic command grammar + fallback behavior, and validate with an eval set before rollout.

## What Is Guaranteed vs Experimental

## Guaranteed (documented)

1. **Groq Speech-to-Text**
- Endpoint is transcription-focused.
- `language` improves accuracy and latency.
- `prompt` guides style/spelling context.
- This does not document full instruction-following rewrite behavior from spoken commands.

2. **OpenAI Speech-to-Text (`/v1/audio/transcriptions`)**
- `gpt-4o-transcribe` and `gpt-4o-mini-transcribe` support prompting.
- Docs describe prompt use as improving transcription quality/context, not as a guaranteed rewrite controller.

3. **Gemini Audio Understanding**
- Can analyze audio and generate structured text outputs in one call.
- Docs explicitly note this API path is not for real-time transcription.

4. **OpenAI Audio + Voice Architecture Guidance**
- OpenAI documents a reliable chained pattern:
  `gpt-4o-transcribe -> text LLM -> gpt-4o-mini-tts`
- This is the predictable architecture when you need control.

## Experimental / inferred

1. **"Spoken command in same audio" -> formatting mode switch in one pass**
- Likely achievable with multimodal LLM audio understanding.
- Not explicitly guaranteed by STT endpoint docs as a contract.
- Must be validated with task-specific evals.

## Critique of V1 Document

1. It correctly identified uncertainty around spoken in-audio commands.
2. It over-weighted STT prompting as if it implies full rewrite control; current docs frame prompting as transcription quality/context guidance.
3. Model naming/versioning in parts of the document is dated (Gemini families have moved on from only "2.0" framing).
4. Some cost/speed statements should be treated as volatile and sourced only from current official model cards.

## Practical Options for Your Repo

## Option A: Keep Groq STT, Add Command-Aware LLM Pass (recommended for reliability)

Flow:
1. Transcribe with current fast model (`whisper-large-v3-turbo`).
2. Run one LLM pass to:
- extract command from transcript prefix
- remove command text from content
- apply requested style/format
3. Return formatted text.

Pros:
1. Predictable and debuggable.
2. Easy to measure command extraction accuracy.
3. Minimal risk to your current fast STT path.

Cons:
1. Two model calls.
2. Slight extra latency.

## Option B: Single-Call Multimodal Audio Understanding (what you want, experimental)

Flow:
1. Send raw audio to a multimodal model with a fixed system instruction:
- "If user speaks a command prefix, apply it; otherwise default to plain transcript."
2. Model returns formatted output directly.

Pros:
1. One API request.
2. Clean UX.

Cons:
1. Lower determinism.
2. Harder to debug command misses.

## Command Protocol (critical for success)

Use a strict spoken grammar, for example:
1. `"Command: tone professional. Command: format bullet points. Content:"`
2. User then dictates content.

Model/system rules:
1. Only parse commands before `"Content:"`.
2. Ignore command parsing after `"Content:"`.
3. If command is ambiguous, fall back to default mode.
4. Never include command words in final output.

This single change usually improves reliability more than model switching.

## Validation Plan (must-do)

Build an eval set of at least 100 clips:
1. 40 with explicit commands
2. 40 plain dictation
3. 20 adversarial/noisy command cases

Track:
1. Command detection precision/recall
2. Formatting correctness score
3. Hallucinated command rate
4. End-to-end latency p50/p95

Release gate example:
1. Command detection F1 >= 0.95
2. False command triggers <= 1%
3. p95 latency within your product threshold

## Recommendation

1. Ship **Option A** first for production reliability.
2. Run **Option B** as an A/B experiment behind a feature flag.
3. Keep one-click fallback to raw transcript if command confidence is low.

## Notes for Current OneScribe Codebase

1. Your current `GroqTranscriber` path is STT-only and fast.
2. If you stay on Groq STT, add:
- `language`
- `prompt` (domain vocabulary)
- optional post-formatting LLM step
3. If you test single-call behavior, isolate it behind a provider abstraction to avoid lock-in.

## Sources (official docs)

1. OpenAI Speech-to-Text guide: https://platform.openai.com/docs/guides/speech-to-text
2. OpenAI Audio API reference (`createTranscription`): https://platform.openai.com/docs/api-reference/audio/createTranscription
3. OpenAI Audio and speech overview: https://platform.openai.com/docs/guides/audio
4. OpenAI Voice agents architecture guide: https://platform.openai.com/docs/guides/voice-agents
5. Gemini Audio understanding: https://ai.google.dev/gemini-api/docs/audio
6. Gemini model list: https://ai.google.dev/gemini-api/docs/models
7. Groq Speech-to-Text docs: https://console.groq.com/docs/speech-to-text
8. Groq Whisper Large V3 Turbo model card: https://console.groq.com/docs/model/whisper-large-v3-turbo
9. Groq Distil Whisper model card: https://console.groq.com/docs/model/distil-whisper-large-v3-en
