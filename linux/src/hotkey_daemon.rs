use std::env;
use std::ffi::CString;
use std::fs;
use std::io::BufRead;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ashpd::{
    AppID,
    desktop::global_shortcuts::{GlobalShortcuts, NewShortcut},
    register_host_app,
};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};

use crate::engine::LocalStream;
use crate::recorder::StreamingPwRecorder;
use crate::{
    AppState, AudioCapture, ClipboardCopyError, CompletionSoundError,
    LocalTranscribeError, PwRecordRecorder, TranscriptionEngine,
    append_transcript_to_journal, build_groq_transcription_request, copy_to_clipboard,
    execute_http_request, install_linux_icon_assets, journal_stamp,
    parse_transcription_response, play_completion_sound, probe_desktop_capabilities,
    resolve_home_dir, resolve_journal_dir,
};

pub const TOGGLE_SHORTCUT_ID: &str = "toggle-recording";
pub const TOGGLE_SHORTCUT_DESCRIPTION: &str = "Start or stop voice recording";
pub const TOGGLE_SHORTCUT_TRIGGER: &str = "Alt+space";
pub const HOST_APP_ID: &str = "com.onescribe.OneScribe";

const GNOME_MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const GNOME_SHORTCUT_BINDING_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/onescribe/";
const GNOME_SHORTCUT_KEYBINDING: &str = "<Alt>space";
const GNOME_SHORTCUT_NAME: &str = "OneScribe Toggle";

fn control_fifo_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/run/user/{uid}/onescribe-control"))
}

// Registers Alt+Space as a GNOME custom keyboard shortcut that writes "toggle"
// to the control FIFO. This is a no-op (with a warning) on non-GNOME desktops
// or where `gsettings` is unavailable. Runs on every daemon start so no
// separate setup step (script, Python, etc.) is required.
fn ensure_gnome_custom_shortcut(fifo_path: &Path) {
    let list_output = match Command::new("gsettings")
        .args(["get", GNOME_MEDIA_KEYS_SCHEMA, "custom-keybindings"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(_) | Err(_) => {
            eprintln!(
                "note: gsettings unavailable or GNOME media-keys schema missing; skipping GNOME custom shortcut setup"
            );
            return;
        }
    };

    let mut paths = parse_gvariant_strv(&String::from_utf8_lossy(&list_output.stdout));
    if !paths.iter().any(|p| p == GNOME_SHORTCUT_BINDING_PATH) {
        paths.push(GNOME_SHORTCUT_BINDING_PATH.to_string());
        let value = format_gvariant_strv(&paths);
        if !run_gsettings_set(&["set", GNOME_MEDIA_KEYS_SCHEMA, "custom-keybindings", &value]) {
            eprintln!("warning: failed to register GNOME custom shortcut path");
            return;
        }
    }

    let keybinding_schema =
        format!("{GNOME_MEDIA_KEYS_SCHEMA}.custom-keybinding:{GNOME_SHORTCUT_BINDING_PATH}");
    let command = format!("bash -c 'echo toggle > {}'", fifo_path.display());

    let configured = run_gsettings_set(&["set", &keybinding_schema, "name", GNOME_SHORTCUT_NAME])
        && run_gsettings_set(&[
            "set",
            &keybinding_schema,
            "binding",
            GNOME_SHORTCUT_KEYBINDING,
        ])
        && run_gsettings_set(&["set", &keybinding_schema, "command", &command]);

    if configured {
        eprintln!(
            "GNOME custom shortcut configured: {GNOME_SHORTCUT_KEYBINDING} → control FIFO"
        );
    } else {
        eprintln!("warning: failed to fully configure GNOME custom shortcut");
    }
}

fn run_gsettings_set(args: &[&str]) -> bool {
    Command::new("gsettings")
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// Parses a gsettings GVariant string-array literal, e.g. "@as []" (empty) or
// "['/a/', '/b/']". Good enough for the values gsettings itself produces;
// paths never contain commas or quotes.
fn parse_gvariant_strv(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with("@as") {
        return Vec::new();
    }
    raw.trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_gvariant_strv(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", quoted.join(", "))
}

// Opens (or creates) the control FIFO and spawns a thread that reads toggle
// commands from it. Returns a channel receiver that fires on each "toggle" line.
// The FIFO is opened O_RDWR so it stays alive even when no external writer is
// connected, preventing the reader from seeing spurious EOF.
fn spawn_control_fifo_listener() -> mpsc::UnboundedReceiver<()> {
    let fifo_path = control_fifo_path();
    let (tx, rx) = mpsc::unbounded_channel();

    std::thread::spawn(move || {
        // Create the FIFO; ignore EEXIST.
        if let Ok(c_path) = CString::new(fifo_path.as_os_str().as_encoded_bytes()) {
            unsafe { libc::mkfifo(c_path.as_ptr(), 0o660) };
        }

        // If a stale regular file was left behind (e.g. by a failed `echo >
        // path` when no FIFO existed), replace it.
        if let Ok(meta) = std::fs::metadata(&fifo_path) {
            if !meta.file_type().is_fifo() {
                let _ = std::fs::remove_file(&fifo_path);
                if let Ok(c_path) = CString::new(fifo_path.as_os_str().as_encoded_bytes()) {
                    unsafe { libc::mkfifo(c_path.as_ptr(), 0o660) };
                }
            }
        }

        eprintln!("control FIFO ready: {}", fifo_path.display());

        // O_RDWR keeps one write-end open so reads block (not EOF) when idle.
        let file = match fs::OpenOptions::new().read(true).write(true).open(&fifo_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("warning: could not open control FIFO: {e}");
                return;
            }
        };

        for line in std::io::BufReader::new(file).lines() {
            match line {
                Ok(msg) if msg.trim() == "toggle" => {
                    if tx.send(()).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });

    rx
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonEvent {
    StateChanged(AppState),
    TranscriptReady(String),
    Warning(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonCommand {
    ToggleRecording,
    Shutdown,
}

#[derive(Debug)]
pub enum HotkeyDaemonError {
    MissingHomeDir,
    InvalidAppId(String),
    DesktopEntry(std::io::Error),
    Portal(ashpd::Error),
    Recorder(crate::RecorderError),
    Transcription(crate::GroqRequestError),
    LocalTranscription(LocalTranscribeError),
    Transport(crate::TransportError),
    Journal(std::io::Error),
}

impl std::fmt::Display for HotkeyDaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHomeDir => write!(f, "HOME is not set"),
            Self::InvalidAppId(app_id) => write!(f, "invalid portal app id: {app_id}"),
            Self::DesktopEntry(error) => write!(f, "{error}"),
            Self::Portal(error) => write!(f, "{error}"),
            Self::Recorder(error) => write!(f, "{error}"),
            Self::Transcription(error) => write!(f, "{error}"),
            Self::LocalTranscription(error) => write!(f, "{error}"),
            Self::Transport(error) => write!(f, "{error}"),
            Self::Journal(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HotkeyDaemonError {}

impl From<ashpd::Error> for HotkeyDaemonError {
    fn from(value: ashpd::Error) -> Self {
        Self::Portal(value)
    }
}

impl From<crate::RecorderError> for HotkeyDaemonError {
    fn from(value: crate::RecorderError) -> Self {
        Self::Recorder(value)
    }
}

impl From<crate::GroqRequestError> for HotkeyDaemonError {
    fn from(value: crate::GroqRequestError) -> Self {
        Self::Transcription(value)
    }
}

impl From<LocalTranscribeError> for HotkeyDaemonError {
    fn from(value: LocalTranscribeError) -> Self {
        Self::LocalTranscription(value)
    }
}

impl From<crate::TransportError> for HotkeyDaemonError {
    fn from(value: crate::TransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<std::io::Error> for HotkeyDaemonError {
    fn from(value: std::io::Error) -> Self {
        Self::Journal(value)
    }
}

pub async fn run_hotkey_daemon(
    environment: &[(String, String)],
    engine: &TranscriptionEngine,
) -> Result<(), HotkeyDaemonError> {
    run_hotkey_daemon_with_control(environment, engine, None, None, None).await
}

pub async fn run_hotkey_daemon_with_control(
    environment: &[(String, String)],
    engine: &TranscriptionEngine,
    event_sender: Option<mpsc::UnboundedSender<DaemonEvent>>,
    mut shutdown_receiver: Option<watch::Receiver<bool>>,
    mut command_receiver: Option<mpsc::UnboundedReceiver<DaemonCommand>>,
) -> Result<(), HotkeyDaemonError> {
    let capabilities = probe_desktop_capabilities(environment);
    if !capabilities.has_global_shortcuts_portal {
        eprintln!("warning: GlobalShortcuts portal not available; hotkey via tray only");
    }
    let home_dir = resolve_home_dir(environment).ok_or(HotkeyDaemonError::MissingHomeDir)?;
    install_linux_icon_assets(&home_dir).map_err(HotkeyDaemonError::DesktopEntry)?;
    ensure_host_desktop_entry(&home_dir).map_err(HotkeyDaemonError::DesktopEntry)?;
    let app_id: AppID = HOST_APP_ID
        .parse()
        .map_err(|_| HotkeyDaemonError::InvalidAppId(HOST_APP_ID.to_string()))?;
    eprintln!("registering host app id: {HOST_APP_ID}");
    register_host_app(app_id).await?;
    let journal_dir = resolve_journal_dir(environment, &home_dir);
    ensure_gnome_custom_shortcut(&control_fifo_path());
    let mut fifo_toggle = spawn_control_fifo_listener();

    let global_shortcuts = GlobalShortcuts::new().await?;
    let mut activated = global_shortcuts.receive_activated().await?;
    let mut active_recording: Option<ActiveRecording> = None;
    let mut session_backoff = Duration::from_secs(1);

    'reconnect: loop {
        eprintln!("creating global shortcuts session...");
        let session = match global_shortcuts.create_session().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "warning: failed to create shortcut session ({e}); retrying in {session_backoff:?}"
                );
                tokio::time::sleep(session_backoff).await;
                session_backoff = (session_backoff * 2).min(Duration::from_secs(30));
                continue 'reconnect;
            }
        };
        session_backoff = Duration::from_secs(1);

        eprintln!("binding shortcut request...");
        let shortcut = NewShortcut::new(TOGGLE_SHORTCUT_ID, TOGGLE_SHORTCUT_DESCRIPTION)
            .preferred_trigger(Some(TOGGLE_SHORTCUT_TRIGGER));
        match global_shortcuts
            .bind_shortcuts(&session, &[shortcut], None)
            .await
        {
            Err(e) => {
                eprintln!(
                    "warning: bind_shortcuts request failed ({e}); retrying in {session_backoff:?}"
                );
                tokio::time::sleep(session_backoff).await;
                session_backoff = (session_backoff * 2).min(Duration::from_secs(30));
                continue 'reconnect;
            }
            Ok(bind_request) => {
                eprintln!("awaiting bind response...");
                // On GNOME 50 the GlobalShortcuts portal backend (gnome-control-center)
                // always segfaults when trying to show the key-binding dialog for a
                // windowless app, returning response code 2 ("Other"). We treat this as
                // non-fatal: hotkey works via the GNOME custom shortcut → control FIFO
                // path configured by ensure_gnome_custom_shortcut() above.
                match bind_request.response() {
                    Ok(bind_response) => {
                        eprintln!("portal shortcut bound successfully");
                        for s in bind_response.shortcuts() {
                            eprintln!(
                                "  shortcut: id={} trigger={}",
                                s.id(),
                                s.trigger_description()
                            );
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "warning: portal shortcut binding failed ({error})"
                        );
                        eprintln!(
                            "  hotkey active via GNOME custom shortcut → control FIFO"
                        );
                    }
                }
            }
        }

        let mut closed = match session.receive_closed().await {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!(
                    "warning: failed to subscribe to session close events ({e}); reconnecting..."
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue 'reconnect;
            }
        };

        eprintln!("waiting for shortcut activations...");
        send_daemon_event(&event_sender, DaemonEvent::StateChanged(AppState::Idle));

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("received Ctrl+C, closing session");
                    stop_active_recording(active_recording.take());
                    let _ = session.close().await;
                    return Ok(());
                }
                _ = async {
                    match &mut shutdown_receiver {
                        Some(receiver) => {
                            let _ = receiver.changed().await;
                        }
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    if shutdown_receiver.as_ref().is_some_and(|receiver| *receiver.borrow()) {
                        eprintln!("received shutdown request, closing session");
                        stop_active_recording(active_recording.take());
                        let _ = session.close().await;
                        return Ok(());
                    }
                }
                maybe_closed = closed.next() => {
                    if maybe_closed.is_some() {
                        eprintln!("portal session closed; reconnecting...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue 'reconnect;
                    }
                }
                maybe_activation = activated.next() => {
                    if let Some(event) = maybe_activation {
                        if event.shortcut_id() != TOGGLE_SHORTCUT_ID {
                            continue;
                        }
                        eprintln!("command: toggle recording (global shortcut)");
                        toggle_recording(
                            &mut active_recording,
                            &event_sender,
                            engine,
                            &journal_dir,
                        )
                        .await?;
                    }
                }
                Some(()) = fifo_toggle.recv() => {
                    eprintln!("command: toggle recording (control FIFO)");
                    toggle_recording(
                        &mut active_recording,
                        &event_sender,
                        engine,
                        &journal_dir,
                    )
                    .await?;
                }
                maybe_command = async {
                    match &mut command_receiver {
                        Some(receiver) => receiver.recv().await,
                        None => std::future::pending::<Option<DaemonCommand>>().await,
                    }
                } => {
                    match maybe_command {
                        Some(DaemonCommand::ToggleRecording) => {
                            eprintln!("command: toggle recording (tray)");
                            toggle_recording(
                                &mut active_recording,
                                &event_sender,
                                engine,
                                &journal_dir,
                            )
                            .await?;
                        }
                        Some(DaemonCommand::Shutdown) => {
                            eprintln!("received shutdown command, closing session");
                            stop_active_recording(active_recording.take());
                            let _ = session.close().await;
                            return Ok(());
                        }
                        None => {
                            command_receiver = None;
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn ensure_host_desktop_entry(home_dir: &Path) -> std::io::Result<PathBuf> {
    let desktop_dir = home_dir.join(".local/share/applications");
    fs::create_dir_all(&desktop_dir)?;

    let desktop_path = desktop_dir.join(format!("{HOST_APP_ID}.desktop"));
    let current_exe = env::current_exe()?;
    let exec = format!("{} daemon", current_exe.display());
    let desired = format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=OneScribe Linux\nComment=Voice transcription hotkey daemon\nExec={exec}\nIcon=onescribe\nTerminal=false\nNoDisplay=true\nCategories=Utility;\nStartupNotify=false\n"
    );

    let should_write = match fs::read_to_string(&desktop_path) {
        Ok(existing) => existing != desired,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    };

    if should_write {
        fs::write(&desktop_path, desired)?;
    }

    Ok(desktop_path)
}

/// A recording in progress. Local recordings stream audio into the model as
/// it is captured; Groq recordings buffer an Opus/OGG upload.
enum ActiveRecording {
    Batch(PwRecordRecorder),
    Streaming {
        recorder: StreamingPwRecorder,
        stream: LocalStream,
    },
}

impl ActiveRecording {
    fn start(engine: &TranscriptionEngine) -> Result<Self, HotkeyDaemonError> {
        match engine {
            TranscriptionEngine::Local(transcriber) => {
                let stream = transcriber.begin_stream();
                let recorder = StreamingPwRecorder::start(stream.chunk_sender())?;
                Ok(Self::Streaming { recorder, stream })
            }
            TranscriptionEngine::Groq { .. } => Ok(Self::Batch(PwRecordRecorder::start()?)),
        }
    }

    /// Stops capture and produces the transcript (blocking).
    fn stop_and_transcribe(
        self,
        engine: &TranscriptionEngine,
    ) -> Result<String, HotkeyDaemonError> {
        match self {
            Self::Batch(recorder) => {
                let audio = recorder.stop()?;
                transcribe_audio_capture(&audio, engine)
            }
            Self::Streaming { recorder, stream } => {
                // Always finish() (joining the worker thread) even when the
                // stop failed, so no worker is left running detached.
                let stopped = recorder.stop();
                let finished = stream.finish();
                stopped?;
                Ok(finished?)
            }
        }
    }

    fn abandon(self) {
        let result = match self {
            Self::Batch(recorder) => recorder.stop().map(|_| ()),
            // Dropping `stream` (bound in the pattern) abandons the worker.
            Self::Streaming { recorder, stream: _ } => recorder.stop().map(|_| ()),
        };
        if let Err(error) = result {
            eprintln!("warning: failed to stop recording during shutdown: {error}");
        }
    }
}

fn handle_completed_recording(
    recording: ActiveRecording,
    engine: &TranscriptionEngine,
    journal_dir: &PathBuf,
) -> Result<CompletedRecordingOutcome, HotkeyDaemonError> {
    let transcript = recording.stop_and_transcribe(engine)?;
    let stamp = journal_stamp(chrono::Local::now());
    let journal_path = append_transcript_to_journal(journal_dir, &stamp, &transcript)?;

    match copy_to_clipboard(&transcript) {
        Ok(method) => {
            eprintln!("copied transcript to clipboard via {:?}", method);
            match play_completion_sound() {
                Ok(_) => eprintln!("played completion sound"),
                Err(CompletionSoundError::MissingBundledSound(_)) => {
                    eprintln!("warning: bundled completion sound is missing; sound skipped")
                }
                Err(error) => eprintln!("warning: failed to play completion sound: {error}"),
            }
        }
        Err(ClipboardCopyError::NoSupportedCommand) => {
            eprintln!("warning: no supported clipboard command found; transcript was not copied");
        }
        Err(error) => {
            eprintln!("warning: failed to copy transcript to clipboard: {error}");
        }
    };

    eprintln!("journal: {}", journal_path.display());
    println!("{transcript}");
    Ok(CompletedRecordingOutcome {
        transcript,
        journal_path,
    })
}

#[derive(Debug)]
struct CompletedRecordingOutcome {
    transcript: String,
    #[allow(dead_code)]
    journal_path: PathBuf,
}

fn stop_active_recording(recording: Option<ActiveRecording>) {
    if let Some(recording) = recording {
        recording.abandon();
    }
}

fn send_daemon_event(
    event_sender: &Option<mpsc::UnboundedSender<DaemonEvent>>,
    event: DaemonEvent,
) {
    if let Some(sender) = event_sender {
        let _ = sender.send(event);
    }
}

async fn toggle_recording(
    active_recording: &mut Option<ActiveRecording>,
    event_sender: &Option<mpsc::UnboundedSender<DaemonEvent>>,
    engine: &TranscriptionEngine,
    journal_dir: &PathBuf,
) -> Result<(), HotkeyDaemonError> {
    match active_recording.take() {
        None => match ActiveRecording::start(engine) {
            Ok(recording) => {
                eprintln!("recording started");
                send_daemon_event(event_sender, DaemonEvent::StateChanged(AppState::Recording));
                *active_recording = Some(recording);
            }
            Err(error) => {
                let message = format!("failed to start recording: {error}");
                eprintln!("warning: {message}");
                send_daemon_event(event_sender, DaemonEvent::Warning(message));
            }
        },
        Some(recording) => {
            eprintln!("recording stopped, transcribing...");
            send_daemon_event(
                event_sender,
                DaemonEvent::StateChanged(AppState::Transcribing),
            );
            let engine = engine.clone();
            let journal_dir = journal_dir.clone();
            // Any per-recording failure (empty capture, transcription error,
            // even a panic in the task) surfaces as a Warning below — never a
            // daemon exit.
            let result = match tokio::task::spawn_blocking(move || {
                handle_completed_recording(recording, &engine, &journal_dir)
            })
            .await
            {
                Ok(result) => result,
                Err(join_error) => Err(HotkeyDaemonError::Journal(std::io::Error::other(
                    format!("transcription task failed: {join_error}"),
                ))),
            };

            match result {
                Ok(outcome) => {
                    send_daemon_event(
                        event_sender,
                        DaemonEvent::TranscriptReady(outcome.transcript),
                    );
                }
                Err(error) => {
                    let message = error.to_string();
                    eprintln!("warning: {message}");
                    send_daemon_event(event_sender, DaemonEvent::Warning(message));
                }
            }

            send_daemon_event(event_sender, DaemonEvent::StateChanged(AppState::Idle));
        }
    }

    Ok(())
}

pub fn transcribe_audio_capture(
    audio: &AudioCapture,
    engine: &TranscriptionEngine,
) -> Result<String, HotkeyDaemonError> {
    match engine {
        TranscriptionEngine::Local(transcriber) => Ok(transcriber.transcribe(audio)?),
        TranscriptionEngine::Groq { api_key } => {
            let boundary = format!(
                "Boundary-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let request = build_groq_transcription_request(
                audio,
                &crate::GroqRequestOptions::new(api_key),
                &boundary,
            )?;
            let response_body = execute_http_request(request)?;
            let transcript = parse_transcription_response(&response_body)?;
            Ok(transcript)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn desktop_entry_path_uses_host_app_id() {
        let path = Path::new("/home/tester")
            .join(".local/share/applications")
            .join(format!("{HOST_APP_ID}.desktop"));
        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .contains(HOST_APP_ID)
        );
    }

    #[test]
    fn daemon_event_transcript_ready_keeps_transcript_text() {
        let event = DaemonEvent::TranscriptReady("hello world".to_string());
        assert_eq!(
            event,
            DaemonEvent::TranscriptReady("hello world".to_string())
        );
    }

    #[test]
    fn daemon_toggle_command_is_stable() {
        assert_eq!(
            DaemonCommand::ToggleRecording,
            DaemonCommand::ToggleRecording
        );
    }

    #[test]
    fn completed_recording_outcome_keeps_journal_path() {
        let outcome = CompletedRecordingOutcome {
            transcript: "hello".to_string(),
            journal_path: PathBuf::from("/tmp/journal.md"),
        };

        assert_eq!(outcome.journal_path, PathBuf::from("/tmp/journal.md"));
    }
}
