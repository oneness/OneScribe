use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;

/// At 8 KiB per recorder chunk this allows roughly eight seconds of audio to
/// wait for inference.  An unbounded channel can consume all available memory
/// if transcription falls behind capture (for example under heavy system
/// load), so backpressure is intentional here.
const STREAM_CHANNEL_CAPACITY: usize = 32;

use transcribe_cpp::{Model, RunOptions, Session, StreamOptions};

use crate::AudioCapture;

/// Default local model: Nemotron Speech Streaming English 0.6B
/// (cache-aware streaming + offline batch), Q8_0 quant.
pub const DEFAULT_LOCAL_MODEL_FILE: &str = "nemotron-speech-streaming-en-0.6b-Q8_0.gguf";
pub const DEFAULT_LOCAL_MODEL_URL: &str = "https://huggingface.co/handy-computer/nemotron-speech-streaming-en-0.6b-gguf/resolve/main/nemotron-speech-streaming-en-0.6b-Q8_0.gguf";

/// Which transcription implementation the app runs with. Chosen once at
/// startup from config; everything downstream just calls `transcribe`.
#[derive(Clone)]
pub enum TranscriptionEngine {
    Groq { api_key: String },
    Local(LocalTranscriber),
}

#[derive(Debug)]
pub enum LocalTranscribeError {
    ModelLoad {
        path: PathBuf,
        source: transcribe_cpp::Error,
    },
    Engine(transcribe_cpp::Error),
    DecoderMissing,
    DecoderSpawn(std::io::Error),
    DecoderIo(std::io::Error),
    DecoderFailed(ExitStatus),
    EmptyAudio,
    EmptyTranscript,
}

impl std::fmt::Display for LocalTranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelLoad { path, source } => {
                write!(f, "failed to load model {}: {source}", path.display())
            }
            Self::Engine(error) => write!(f, "local transcription failed: {error}"),
            Self::DecoderMissing => {
                write!(f, "ffmpeg is required to decode audio for the local engine")
            }
            Self::DecoderSpawn(error) => write!(f, "failed to start ffmpeg decoder: {error}"),
            Self::DecoderIo(error) => write!(f, "ffmpeg decode failed: {error}"),
            Self::DecoderFailed(status) => {
                write!(f, "ffmpeg decoder exited unsuccessfully: {status}")
            }
            Self::EmptyAudio => write!(f, "decoded audio is empty"),
            Self::EmptyTranscript => write!(f, "local transcription produced no text"),
        }
    }
}

impl std::error::Error for LocalTranscribeError {}

/// A loaded local model. `Model` is `Send + Sync` and Arc-backed, so this is
/// cheap to clone into worker tasks; a fresh `Session` is created per run.
#[derive(Clone)]
pub struct LocalTranscriber {
    model: Model,
    language: Option<String>,
}

impl LocalTranscriber {
    pub fn load(model_path: &Path, language: Option<String>) -> Result<Self, LocalTranscribeError> {
        let model = Model::load(model_path).map_err(|source| LocalTranscribeError::ModelLoad {
            path: model_path.to_path_buf(),
            source,
        })?;
        Ok(Self { model, language })
    }

    fn run_options(&self) -> RunOptions {
        RunOptions {
            language: self.language.clone(),
            ..RunOptions::default()
        }
    }

    pub fn transcribe(&self, audio: &AudioCapture) -> Result<String, LocalTranscribeError> {
        let pcm = decode_capture_with_ffmpeg(audio)?;
        let mut session = self.model.session().map_err(LocalTranscribeError::Engine)?;
        run_batch(&mut session, &pcm, &self.run_options())
    }

    /// Starts a live transcription: a worker thread owns the model session and
    /// consumes raw s16le chunks as they are recorded, so the transcript is
    /// essentially ready the moment recording stops. Models without streaming
    /// support fall back to a batch run over the accumulated audio.
    pub fn begin_stream(&self) -> LocalStream {
        let (chunk_sender, chunk_receiver) = mpsc::sync_channel::<Vec<u8>>(STREAM_CHANNEL_CAPACITY);
        let model = self.model.clone();
        let run_options = self.run_options();
        let worker =
            std::thread::spawn(move || stream_worker(&model, &run_options, &chunk_receiver));
        LocalStream {
            chunk_sender,
            worker,
        }
    }
}

/// An in-flight streaming transcription. Feed it via the sender from
/// [`LocalStream::chunk_sender`]; `finish()` waits for the final text.
pub struct LocalStream {
    chunk_sender: mpsc::SyncSender<Vec<u8>>,
    worker: JoinHandle<Result<String, LocalTranscribeError>>,
}

impl LocalStream {
    pub fn chunk_sender(&self) -> mpsc::SyncSender<Vec<u8>> {
        self.chunk_sender.clone()
    }

    /// Signals end of audio and returns the final transcript.
    pub fn finish(self) -> Result<String, LocalTranscribeError> {
        // Dropping the last sender ends the worker's receive loop. The
        // recorder's clone must already be dropped (recorder stopped first).
        let Self {
            chunk_sender,
            worker,
        } = self;
        drop(chunk_sender);
        worker.join().map_err(|_| {
            LocalTranscribeError::Engine(transcribe_cpp::Error::Busy(
                "streaming worker panicked".into(),
            ))
        })?
    }
}

fn stream_worker(
    model: &Model,
    run_options: &RunOptions,
    chunks: &mpsc::Receiver<Vec<u8>>,
) -> Result<String, LocalTranscribeError> {
    let mut session = model.session().map_err(LocalTranscribeError::Engine)?;

    match session.stream(run_options, &StreamOptions::default()) {
        Ok(mut stream) => {
            for bytes in chunks {
                stream
                    .feed(&pcm_f32_from_s16le(&bytes))
                    .map_err(LocalTranscribeError::Engine)?;
            }
            stream.finalize().map_err(LocalTranscribeError::Engine)?;
            return non_empty_text(stream.text().full);
        }
        // Offline-only models (e.g. Parakeet TDT): fall through to batch.
        Err(transcribe_cpp::Error::NotImplemented(_) | transcribe_cpp::Error::Unsupported(_)) => {
            eprintln!("model does not support streaming; falling back to batch transcription");
        }
        Err(error) => return Err(LocalTranscribeError::Engine(error)),
    }

    let mut pcm = Vec::new();
    for bytes in chunks {
        pcm.extend(pcm_f32_from_s16le(&bytes));
    }
    run_batch(&mut session, &pcm, run_options)
}

fn run_batch(
    session: &mut Session,
    pcm: &[f32],
    options: &RunOptions,
) -> Result<String, LocalTranscribeError> {
    if pcm.is_empty() {
        return Err(LocalTranscribeError::EmptyAudio);
    }
    let transcript = session
        .run(pcm, options)
        .map_err(LocalTranscribeError::Engine)?;
    non_empty_text(transcript.text)
}

fn non_empty_text(text: String) -> Result<String, LocalTranscribeError> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(LocalTranscribeError::EmptyTranscript);
    }
    Ok(text)
}

fn pcm_f32_from_s16le(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
        .collect()
}

fn pcm_f32_from_f32le(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|quad| f32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
        .collect()
}

/// Produces 16 kHz mono f32 PCM from a batch capture. Every batch capture
/// comes from a file on disk (recorder OGG output or a CLI-supplied file), and
/// ffmpeg needs a seekable input for containers like m4a anyway.
fn decode_capture_with_ffmpeg(audio: &AudioCapture) -> Result<Vec<f32>, LocalTranscribeError> {
    let input_path = audio
        .temp_path
        .as_ref()
        .filter(|path| path.exists())
        .ok_or_else(|| {
            LocalTranscribeError::DecoderIo(std::io::Error::other(
                "no on-disk audio file to decode",
            ))
        })?;

    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(input_path)
        .args(["-f", "f32le", "-ar", "16000", "-ac", "1", "pipe:1"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => LocalTranscribeError::DecoderMissing,
            _ => LocalTranscribeError::DecoderSpawn(error),
        })?;

    if !output.status.success() {
        return Err(LocalTranscribeError::DecoderFailed(output.status));
    }

    Ok(pcm_f32_from_f32le(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s16le_conversion_maps_full_scale_to_unit_range() {
        let bytes = [
            0x00, 0x00, // 0
            0xff, 0x7f, // i16::MAX
            0x00, 0x80, // i16::MIN
        ];
        let pcm = pcm_f32_from_s16le(&bytes);

        assert_eq!(pcm.len(), 3);
        assert_eq!(pcm[0], 0.0);
        assert!((pcm[1] - (32767.0 / 32768.0)).abs() < f32::EPSILON);
        assert_eq!(pcm[2], -1.0);
    }

    #[test]
    fn s16le_conversion_ignores_trailing_odd_byte() {
        let pcm = pcm_f32_from_s16le(&[0x00, 0x00, 0x7f]);
        assert_eq!(pcm.len(), 1);
    }

    #[test]
    fn f32le_roundtrip() {
        let samples = [0.5f32, -0.25, 1.0];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(pcm_f32_from_f32le(&bytes), samples);
    }

    // Exercises the CLI batch route (audio file -> ffmpeg decode -> model)
    // against a real model. Run with:
    //   ONESCRIBE_TEST_MODEL=~/.local/share/onescribe/models/<model>.gguf \
    //   ONESCRIBE_TEST_AUDIO=/path/to/speech.wav \
    //   cargo test --release -- --ignored transcribes_speech
    #[test]
    #[ignore = "requires ONESCRIBE_TEST_MODEL and ONESCRIBE_TEST_AUDIO"]
    fn file_capture_transcribes_speech() {
        let model_path = std::env::var("ONESCRIBE_TEST_MODEL").expect("ONESCRIBE_TEST_MODEL");
        let audio_path = std::env::var("ONESCRIBE_TEST_AUDIO").expect("ONESCRIBE_TEST_AUDIO");
        let bytes = std::fs::read(&audio_path).expect("read audio sample");

        let audio = AudioCapture::new(
            "sample.wav",
            "audio/x-wav",
            bytes,
            Some(PathBuf::from(&audio_path)),
        );
        let transcriber =
            LocalTranscriber::load(Path::new(&model_path), test_language()).expect("load model");
        let transcript = transcriber.transcribe(&audio).expect("transcribe");

        eprintln!("transcript: {transcript}");
        assert!(transcript.to_lowercase().contains("country"));
    }

    // Exercises the daemon's streaming route: raw s16le fed in recorder-sized
    // chunks while "recording", final text on finish. Same env vars as above.
    #[test]
    #[ignore = "requires ONESCRIBE_TEST_MODEL and ONESCRIBE_TEST_S16LE"]
    fn streaming_capture_transcribes_speech() {
        let model_path = std::env::var("ONESCRIBE_TEST_MODEL").expect("ONESCRIBE_TEST_MODEL");
        let audio_path = std::env::var("ONESCRIBE_TEST_S16LE").expect("ONESCRIBE_TEST_S16LE");
        let bytes = std::fs::read(&audio_path).expect("read raw pcm sample");

        let transcriber =
            LocalTranscriber::load(Path::new(&model_path), test_language()).expect("load model");
        let stream = transcriber.begin_stream();
        let sender = stream.chunk_sender();
        for chunk in bytes.chunks(8192) {
            sender.send(chunk.to_vec()).expect("feed chunk");
        }
        drop(sender);
        let transcript = stream.finish().expect("finish stream");

        eprintln!("streamed transcript: {transcript}");
        assert!(transcript.to_lowercase().contains("country"));
    }

    fn test_language() -> Option<String> {
        std::env::var("ONESCRIBE_TEST_LANGUAGE").ok()
    }
}
