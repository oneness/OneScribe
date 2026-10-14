use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::AudioCapture;

pub const DEFAULT_RECORD_SECONDS: u64 = 5;
pub const SOURCE_ENV_KEY: &str = "ONESCRIBE_SOURCE";

#[derive(Debug)]
pub enum RecorderError {
    Spawn(io::Error),
    EncoderMissing(&'static str),
    EncoderSpawn(io::Error),
    Signal(io::Error),
    Wait(io::Error),
    EncoderWait(io::Error),
    EncoderFailed(ExitStatus),
    MissingOutput(PathBuf),
    Read(io::Error),
    EmptyCapture,
}

impl std::fmt::Display for RecorderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "failed to start pw-record: {error}"),
            Self::EncoderMissing(program) => {
                write!(f, "required audio encoder is not installed: {program}")
            }
            Self::EncoderSpawn(error) => write!(f, "failed to start ffmpeg encoder: {error}"),
            Self::Signal(error) => write!(f, "failed to stop pw-record: {error}"),
            Self::Wait(error) => write!(f, "failed while waiting for pw-record: {error}"),
            Self::EncoderWait(error) => write!(f, "failed while waiting for ffmpeg: {error}"),
            Self::EncoderFailed(status) => write!(f, "ffmpeg exited unsuccessfully: {status}"),
            Self::MissingOutput(path) => {
                write!(
                    f,
                    "recording pipeline did not produce output at {}",
                    path.display()
                )
            }
            Self::Read(error) => write!(f, "failed to read recorded audio: {error}"),
            Self::EmptyCapture => write!(f, "recording produced no audio data"),
        }
    }
}

impl std::error::Error for RecorderError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PwRecordCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

impl PwRecordCommand {
    pub fn for_stdout_raw(target: Option<&str>) -> Self {
        let mut args = vec![
            "-".to_string(),
            "--rate".to_string(),
            "16000".to_string(),
            "--channels".to_string(),
            "1".to_string(),
            "--format".to_string(),
            "s16".to_string(),
            "--raw".to_string(),
        ];

        if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
            args.push("--target".to_string());
            args.push(target.to_string());
        }

        Self {
            program: "pw-record",
            args,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegEncodeCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

impl FfmpegEncodeCommand {
    pub fn for_output(output_path: &Path) -> Self {
        Self {
            program: "ffmpeg",
            args: vec![
                "-f".to_string(),
                "s16le".to_string(),
                "-ar".to_string(),
                "16000".to_string(),
                "-ac".to_string(),
                "1".to_string(),
                "-i".to_string(),
                "pipe:0".to_string(),
                "-c:a".to_string(),
                "libopus".to_string(),
                "-b:a".to_string(),
                "16k".to_string(),
                "-application".to_string(),
                "voip".to_string(),
                "-f".to_string(),
                "ogg".to_string(),
                "-y".to_string(),
                output_path.display().to_string(),
            ],
        }
    }
}

/// Buffered capture: pw-record piped into ffmpeg, producing an Opus/OGG file
/// (used for Groq uploads and the one-shot CLI record mode).
pub struct PwRecordRecorder {
    pw_record: Child,
    ffmpeg: Child,
    output_path: PathBuf,
}

/// Spawns pw-record with piped stdout, reaping the child on setup failure.
fn spawn_pw_record() -> Result<(Child, std::process::ChildStdout), RecorderError> {
    let target = env::var(SOURCE_ENV_KEY).ok();
    let record_command = PwRecordCommand::for_stdout_raw(target.as_deref());
    let mut pw_record = Command::new(record_command.program)
        .args(&record_command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(RecorderError::Spawn)?;

    match pw_record.stdout.take() {
        Some(stdout) => Ok((pw_record, stdout)),
        None => {
            let _ = pw_record.kill();
            let _ = pw_record.wait();
            Err(RecorderError::Spawn(io::Error::other(
                "failed to capture pw-record stdout",
            )))
        }
    }
}

/// Stops pw-record and always reaps it: SIGINT for a clean stop, escalating to
/// kill if the signal cannot be delivered, so downstream pipe readers are
/// guaranteed to see EOF even on the error path.
fn stop_pw_record(pw_record: &mut Child) -> Result<(), RecorderError> {
    if let Err(error) = send_sigint(pw_record.id()) {
        let _ = pw_record.kill();
        let _ = pw_record.wait();
        return Err(RecorderError::Signal(error));
    }
    pw_record.wait().map_err(RecorderError::Wait)?;
    Ok(())
}

impl PwRecordRecorder {
    pub fn start() -> Result<Self, RecorderError> {
        let (mut pw_record, pw_stdout) = spawn_pw_record()?;

        let output_path = temp_recording_path();
        let encode_command = FfmpegEncodeCommand::for_output(&output_path);
        let ffmpeg = match Command::new(encode_command.program)
            .args(&encode_command.args)
            .stdin(Stdio::from(pw_stdout))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = pw_record.kill();
                let _ = pw_record.wait();
                return Err(match error.kind() {
                    io::ErrorKind::NotFound => RecorderError::EncoderMissing("ffmpeg"),
                    _ => RecorderError::EncoderSpawn(error),
                });
            }
        };

        Ok(Self {
            pw_record,
            ffmpeg,
            output_path,
        })
    }

    pub fn stop(mut self) -> Result<AudioCapture, RecorderError> {
        if let Err(error) = stop_pw_record(&mut self.pw_record) {
            // pw-record is reaped either way; don't leave ffmpeg behind.
            let _ = self.ffmpeg.kill();
            let _ = self.ffmpeg.wait();
            return Err(error);
        }

        let ffmpeg_status = self.ffmpeg.wait().map_err(RecorderError::EncoderWait)?;
        if !ffmpeg_status.success() {
            return Err(RecorderError::EncoderFailed(ffmpeg_status));
        }

        if !self.output_path.exists() {
            return Err(RecorderError::MissingOutput(self.output_path));
        }

        let bytes = fs::read(&self.output_path).map_err(RecorderError::Read)?;
        let file_name = self
            .output_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("recording.ogg")
            .to_string();
        let mime_type = crate::guess_mime_type(&self.output_path);
        eprintln!(
            "recording saved: path={} mime={} bytes={}",
            self.output_path.display(),
            mime_type,
            bytes.len()
        );
        Ok(AudioCapture::new(
            file_name,
            mime_type,
            bytes,
            Some(self.output_path),
        ))
    }
}

pub fn record_for_duration(duration: Duration) -> Result<AudioCapture, RecorderError> {
    let recorder = PwRecordRecorder::start()?;
    std::thread::sleep(duration);
    recorder.stop()
}

/// Captures raw s16le PCM and forwards it in small chunks to a channel as it
/// arrives (for live streaming transcription) instead of buffering to the end.
pub struct StreamingPwRecorder {
    pw_record: Child,
    reader: std::thread::JoinHandle<io::Result<u64>>,
}

impl StreamingPwRecorder {
    pub fn start(
        chunk_sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    ) -> Result<Self, RecorderError> {
        let (pw_record, mut pw_stdout) = spawn_pw_record()?;

        let reader = std::thread::spawn(move || {
            use std::io::Read;
            // 8192 bytes = 256 ms of 16 kHz mono s16le.
            let mut buffer = [0u8; 8192];
            // Pipe reads can split mid-sample; carry the odd trailing byte so
            // every chunk sent downstream stays aligned to whole s16 samples.
            let mut carry: Option<u8> = None;
            let mut total: u64 = 0;
            loop {
                match pw_stdout.read(&mut buffer) {
                    Ok(0) => return Ok(total),
                    Ok(n) => {
                        total += n as u64;
                        let mut chunk = Vec::with_capacity(n + 1);
                        chunk.extend(carry.take());
                        chunk.extend_from_slice(&buffer[..n]);
                        if chunk.len() % 2 == 1 {
                            carry = chunk.pop();
                        }
                        // A bounded channel applies backpressure when inference
                        // falls behind capture. Blocking this reader is safe:
                        // pw-record then blocks on its pipe instead of letting
                        // queued audio grow until the machine runs out of RAM.
                        if !chunk.is_empty() && chunk_sender.send(chunk).is_err() {
                            // The transcription worker exited. Keep draining so
                            // pw-record can still be stopped and reaped cleanly.
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
        });

        Ok(Self { pw_record, reader })
    }

    /// Stops recording. All captured audio is guaranteed to have been sent to
    /// the channel when this returns; the recorder's sender clone is dropped
    /// with the reader thread.
    pub fn stop(mut self) -> Result<u64, RecorderError> {
        // pw-record is reaped even when stopping errored, so the reader hits
        // EOF; always join it (dropping its channel sender) before returning.
        let stop_result = stop_pw_record(&mut self.pw_record);
        let read_result = self
            .reader
            .join()
            .map_err(|_| RecorderError::Read(io::Error::other("pcm reader thread panicked")));
        stop_result?;
        let bytes = read_result?.map_err(RecorderError::Read)?;
        if bytes == 0 {
            return Err(RecorderError::EmptyCapture);
        }
        eprintln!("recording captured: raw pcm s16le bytes={bytes} (streamed)");
        Ok(bytes)
    }
}

pub fn temp_recording_path() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("onescribe-recording-{suffix}.ogg"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSource {
    pub id: u32,
    pub description: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_audio_sources() -> io::Result<Vec<AudioSource>> {
    let output = Command::new("wpctl").arg("status").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut sources = Vec::new();
    let mut in_sources = false;
    for raw_line in stdout.lines() {
        let line = raw_line.trim_end();
        let normalized = line.trim_start().trim_start_matches('│').trim_start();
        if normalized == "├─ Sources:" {
            in_sources = true;
            continue;
        }
        if in_sources && normalized == "├─ Filters:" {
            break;
        }
        if !in_sources {
            continue;
        }

        let trimmed = normalized;
        if trimmed.is_empty() {
            continue;
        }

        let is_default = trimmed.starts_with('*');
        let entry = trimmed.trim_start_matches('*').trim();
        let Some((id_part, rest)) = entry.split_once('.') else {
            continue;
        };
        let Ok(id) = id_part.trim().parse::<u32>() else {
            continue;
        };

        let description = rest
            .split('[')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        let name = inspect_source_name(id).unwrap_or_else(|| description.clone());

        sources.push(AudioSource {
            id,
            description,
            name,
            is_default,
        });
    }

    Ok(sources)
}

fn inspect_source_name(id: u32) -> Option<String> {
    let output = Command::new("wpctl")
        .args(["inspect", &id.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("* node.name = ")
                .map(|value| value.trim_matches('"').to_string())
        })
}

fn send_sigint(pid: u32) -> io::Result<()> {
    let result = unsafe { libc::kill(pid as i32, libc::SIGINT) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pw_record_command_matches_expected_audio_settings() {
        let command = PwRecordCommand::for_stdout_raw(None);

        assert_eq!(command.program, "pw-record");
        assert_eq!(
            command.args,
            vec![
                "-",
                "--rate",
                "16000",
                "--channels",
                "1",
                "--format",
                "s16",
                "--raw",
            ]
        );
    }

    #[test]
    fn pw_record_command_includes_target_when_provided() {
        let command = PwRecordCommand::for_stdout_raw(Some("alsa_input.test"));

        assert_eq!(
            command.args,
            vec![
                "-",
                "--rate",
                "16000",
                "--channels",
                "1",
                "--format",
                "s16",
                "--raw",
                "--target",
                "alsa_input.test",
            ]
        );
    }

    #[test]
    fn ffmpeg_encode_command_matches_expected_audio_settings() {
        let output = Path::new("/tmp/test.ogg");
        let command = FfmpegEncodeCommand::for_output(output);

        assert_eq!(command.program, "ffmpeg");
        assert_eq!(
            command.args,
            vec![
                "-f",
                "s16le",
                "-ar",
                "16000",
                "-ac",
                "1",
                "-i",
                "pipe:0",
                "-c:a",
                "libopus",
                "-b:a",
                "16k",
                "-application",
                "voip",
                "-f",
                "ogg",
                "-y",
                "/tmp/test.ogg",
            ]
        );
    }

    #[test]
    fn temp_recording_path_uses_ogg_extension() {
        let path = temp_recording_path();
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("ogg"));
        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .starts_with("onescribe-recording-")
        );
    }
}
