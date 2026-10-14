use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;

use chrono::Local;
use onescribe_linux_core::{
    AudioCapture, ClipboardCopyError, Config, DEFAULT_LOCAL_MODEL_FILE, DEFAULT_LOCAL_MODEL_URL,
    DEFAULT_RECORD_SECONDS, LocalTranscriber, SOURCE_ENV_KEY, TranscriptionBackendChoice,
    TranscriptionEngine, copy_to_clipboard, current_environment, guess_mime_type, journal_stamp,
    list_audio_sources, load_config_json, play_completion_sound, record_for_duration,
    resolve_config_path, resolve_home_dir, resolve_journal_dir, run_tray_daemon,
    transcribe_audio_capture,
};

enum CliCommand {
    TranscribeFile(PathBuf),
    RecordThenTranscribe { seconds: u64 },
    Daemon,
    Sources,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command = parse_command(std::env::args_os())?;
    let environment = current_environment();
    if matches!(command, CliCommand::Sources) {
        print_sources()?;
        return Ok(());
    }

    let home_dir = resolve_home_dir(&environment).ok_or("HOME is not set")?;
    let config_path = resolve_config_path(&environment, &home_dir);
    let config_json = load_config_json(&config_path)?;
    let config = Config::resolve(&environment, config_json.as_deref(), &home_dir);

    let engine = match config.backend {
        TranscriptionBackendChoice::Local => {
            if !config.model_path.exists() {
                let is_default_model = config.model_path.file_name().and_then(|n| n.to_str())
                    == Some(DEFAULT_LOCAL_MODEL_FILE);
                if !is_default_model {
                    return Err(format!(
                        "local model not found: {} (custom \"model_path\" — download it manually, or remove the key to use the default model, which downloads automatically)",
                        config.model_path.display(),
                    )
                    .into());
                }
                download_default_model(&config.model_path).map_err(|error| {
                    format!(
                        "model download failed: {error}\nrerun to resume, or download manually:\n  curl -L -o '{}' '{}'",
                        config.model_path.display(),
                        DEFAULT_LOCAL_MODEL_URL,
                    )
                })?;
            }
            eprintln!(
                "transcription backend: local ({}, language: {})",
                config.model_path.display(),
                config.language.as_deref().unwrap_or("auto")
            );
            let load_started = std::time::Instant::now();
            let transcriber = LocalTranscriber::load(&config.model_path, config.language)?;
            eprintln!("local model loaded in {:.1?}", load_started.elapsed());
            TranscriptionEngine::Local(transcriber)
        }
        TranscriptionBackendChoice::Groq => {
            let api_key = config.groq_api_key.ok_or_else(|| {
                format!(
                    "Groq API key not configured. Set GROQ_API_KEY or create {} (or set \"backend\": \"local\" to transcribe on-device)",
                    config_path.display()
                )
            })?;
            eprintln!("transcription backend: groq (cloud)");
            TranscriptionEngine::Groq { api_key }
        }
    };

    if matches!(command, CliCommand::Daemon) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(run_tray_daemon(&environment, &engine))?;
        return Ok(());
    }

    let audio = match command {
        CliCommand::TranscribeFile(audio_path) => load_audio_capture(&audio_path)?,
        CliCommand::RecordThenTranscribe { seconds } => {
            eprintln!("recording for {seconds} seconds...");
            record_for_duration(Duration::from_secs(seconds))?
        }
        CliCommand::Daemon => unreachable!("daemon mode returns before audio loading"),
        CliCommand::Sources => unreachable!("sources mode returns before audio loading"),
    };

    let transcript = transcribe_audio_capture(&audio, &engine)?;

    let journal_dir = resolve_journal_dir(&environment, &home_dir);
    let stamp = journal_stamp(Local::now());
    let journal_path =
        onescribe_linux_core::append_transcript_to_journal(&journal_dir, &stamp, &transcript)?;

    match copy_to_clipboard(&transcript) {
        Ok(method) => {
            eprintln!("copied transcript to clipboard via {:?}", method);
            if let Err(error) = play_completion_sound() {
                eprintln!("warning: failed to play completion sound: {error}");
            }
        }
        Err(ClipboardCopyError::NoSupportedCommand) => {
            eprintln!("warning: no supported clipboard command found; transcript was not copied")
        }
        Err(error) => eprintln!("warning: failed to copy transcript to clipboard: {error}"),
    }

    println!("{transcript}");
    eprintln!("journal: {}", journal_path.display());
    Ok(())
}

/// Downloads the default model on first run: streamed to a `.partial` file
/// (resumed via HTTP Range if one is left over), size-verified, then renamed
/// into place so the final path only ever holds a complete model.
fn download_default_model(dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = dest.parent().ok_or("model path has no parent directory")?;
    fs::create_dir_all(dir)?;
    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid model file name")?;
    let partial_path = dest.with_file_name(format!("{file_name}.partial"));
    let resume_from = fs::metadata(&partial_path)
        .map(|meta| meta.len())
        .unwrap_or(0);

    eprintln!("downloading model (~696 MB, one-time) to {}", dest.display());
    let client = reqwest::blocking::Client::builder()
        // The default 30 s total timeout would abort a large download.
        .timeout(None)
        .connect_timeout(Duration::from_secs(30))
        .build()?;
    let mut request = client.get(DEFAULT_LOCAL_MODEL_URL);
    if resume_from > 0 {
        eprintln!("resuming partial download at {} MB", resume_from >> 20);
        request = request.header("Range", format!("bytes={resume_from}-"));
    }
    let mut response = request.send()?.error_for_status()?;

    // 206 means the server honored the Range; anything else starts over.
    let (mut file, mut downloaded) =
        if resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            (
                fs::OpenOptions::new().append(true).open(&partial_path)?,
                resume_from,
            )
        } else {
            (fs::File::create(&partial_path)?, 0)
        };
    let expected_total = response.content_length().map(|len| downloaded + len);

    let mut buffer = vec![0u8; 1 << 20];
    let mut last_logged = downloaded;
    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        downloaded += n as u64;
        if downloaded - last_logged >= 64 << 20 {
            match expected_total {
                Some(total) => eprintln!(
                    "  {} / {} MB ({}%)",
                    downloaded >> 20,
                    total >> 20,
                    downloaded * 100 / total
                ),
                None => eprintln!("  {} MB", downloaded >> 20),
            }
            last_logged = downloaded;
        }
    }
    file.flush()?;
    drop(file);

    if let Some(total) = expected_total {
        if downloaded != total {
            return Err(format!(
                "connection ended early ({} of {} MB); the partial file is kept",
                downloaded >> 20,
                total >> 20
            )
            .into());
        }
    }
    fs::rename(&partial_path, dest)?;
    eprintln!("model downloaded ({} MB)", downloaded >> 20);
    Ok(())
}

fn load_audio_capture(audio_path: &PathBuf) -> Result<AudioCapture, Box<dyn std::error::Error>> {
    let audio_bytes = fs::read(audio_path)?;
    let file_name = audio_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid audio file name")?;
    Ok(AudioCapture::new(
        file_name,
        guess_mime_type(audio_path),
        audio_bytes,
        Some(audio_path.clone()),
    ))
}

fn parse_command<I>(args: I) -> Result<CliCommand, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    let first = args.next().ok_or(usage())?;

    if first == "record" {
        let seconds = match args.next() {
            Some(value) => value
                .into_string()
                .map_err(|_| usage())?
                .parse::<u64>()
                .map_err(|_| usage())?,
            None => DEFAULT_RECORD_SECONDS,
        };

        if args.next().is_some() {
            return Err(usage().into());
        }

        return Ok(CliCommand::RecordThenTranscribe { seconds });
    }

    if first == "daemon" {
        if args.next().is_some() {
            return Err(usage().into());
        }

        return Ok(CliCommand::Daemon);
    }

    if first == "sources" {
        if args.next().is_some() {
            return Err(usage().into());
        }

        return Ok(CliCommand::Sources);
    }

    if args.next().is_some() {
        return Err(usage().into());
    }

    Ok(CliCommand::TranscribeFile(PathBuf::from(first)))
}

fn usage() -> &'static str {
    "usage: onescribe-linux-core <audio-file-path> | onescribe-linux-core record [seconds] | onescribe-linux-core daemon | onescribe-linux-core sources"
}

fn print_sources() -> Result<(), Box<dyn std::error::Error>> {
    let sources = list_audio_sources()?;
    for source in sources {
        let default_marker = if source.is_default { "*" } else { " " };
        println!(
            "{default_marker} id={} name={} description={}",
            source.id, source.name, source.description
        );
    }
    println!("set {SOURCE_ENV_KEY} to the desired source name before running record or daemon");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_accepts_file_path_mode() {
        let command = parse_command([
            OsString::from("onescribe-linux-core"),
            OsString::from("/tmp/test.wav"),
        ])
        .unwrap();

        match command {
            CliCommand::TranscribeFile(path) => assert_eq!(path, PathBuf::from("/tmp/test.wav")),
            CliCommand::RecordThenTranscribe { .. } | CliCommand::Daemon | CliCommand::Sources => {
                panic!("expected file mode")
            }
        }
    }

    #[test]
    fn parse_command_uses_default_record_duration() {
        let command = parse_command([
            OsString::from("onescribe-linux-core"),
            OsString::from("record"),
        ])
        .unwrap();

        match command {
            CliCommand::RecordThenTranscribe { seconds } => {
                assert_eq!(seconds, DEFAULT_RECORD_SECONDS)
            }
            CliCommand::TranscribeFile(_) | CliCommand::Daemon | CliCommand::Sources => {
                panic!("expected record mode")
            }
        }
    }

    #[test]
    fn parse_command_accepts_custom_record_duration() {
        let command = parse_command([
            OsString::from("onescribe-linux-core"),
            OsString::from("record"),
            OsString::from("7"),
        ])
        .unwrap();

        match command {
            CliCommand::RecordThenTranscribe { seconds } => assert_eq!(seconds, 7),
            CliCommand::TranscribeFile(_) | CliCommand::Daemon | CliCommand::Sources => {
                panic!("expected record mode")
            }
        }
    }

    #[test]
    fn parse_command_accepts_daemon_mode() {
        let command = parse_command([
            OsString::from("onescribe-linux-core"),
            OsString::from("daemon"),
        ])
        .unwrap();

        match command {
            CliCommand::Daemon => {}
            CliCommand::TranscribeFile(_)
            | CliCommand::RecordThenTranscribe { .. }
            | CliCommand::Sources => panic!("expected daemon mode"),
        }
    }

    #[test]
    fn parse_command_accepts_sources_mode() {
        let command = parse_command([
            OsString::from("onescribe-linux-core"),
            OsString::from("sources"),
        ])
        .unwrap();

        match command {
            CliCommand::Sources => {}
            CliCommand::TranscribeFile(_)
            | CliCommand::RecordThenTranscribe { .. }
            | CliCommand::Daemon => panic!("expected sources mode"),
        }
    }
}
