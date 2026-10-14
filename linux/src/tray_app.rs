use ksni::{MenuItem, Status, ToolTip, Tray, TrayMethods, menu::StandardItem};
use tokio::sync::mpsc;

use crate::{
    AppState, DaemonCommand, DaemonEvent, HOST_APP_ID, HotkeyDaemonError, TranscriptionEngine,
    install_linux_icon_assets, resolve_home_dir, run_hotkey_daemon_with_control,
};

pub async fn run_tray_daemon(
    environment: &[(String, String)],
    engine: &TranscriptionEngine,
) -> Result<(), HotkeyDaemonError> {
    let owned_environment = environment.to_vec();
    let owned_engine = engine.clone();
    let home_dir = resolve_home_dir(environment).ok_or(HotkeyDaemonError::MissingHomeDir)?;
    let icon_theme_path =
        install_linux_icon_assets(&home_dir).map_err(HotkeyDaemonError::DesktopEntry)?;
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
    let (command_sender, command_receiver) = mpsc::unbounded_channel();

    let tray_handle = match OneScribeTray::new(
        icon_theme_path.to_string_lossy().into_owned(),
        command_sender,
    )
    .assume_sni_available(true)
    .spawn()
    .await
    {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("warning: failed to start tray icon: {error}; continuing without it");
            None
        }
    };

    let daemon_task = tokio::spawn(async move {
        run_hotkey_daemon_with_control(
            &owned_environment,
            &owned_engine,
            Some(event_sender),
            None,
            Some(command_receiver),
        )
        .await
    });
    tokio::pin!(daemon_task);

    loop {
        tokio::select! {
            daemon_result = &mut daemon_task => {
                if let Some(handle) = &tray_handle {
                    handle.shutdown().await;
                }

                return daemon_result
                    .map_err(|error| HotkeyDaemonError::Journal(std::io::Error::other(error.to_string())))?;
            }
            maybe_event = event_receiver.recv() => {
                match maybe_event {
                    Some(event) => {
                        echo_event_to_terminal(&event);
                        if let Some(handle) = &tray_handle {
                            let _ = handle
                                .update(|tray| tray.apply_event(event.clone()))
                                .await;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    if let Some(handle) = &tray_handle {
        handle.shutdown().await;
    }

    Ok(())
}

struct OneScribeTray {
    app_state: AppState,
    icon_theme_path: String,
    last_transcript_preview: Option<String>,
    last_warning: Option<String>,
    command_sender: mpsc::UnboundedSender<DaemonCommand>,
}

impl OneScribeTray {
    fn new(icon_theme_path: String, command_sender: mpsc::UnboundedSender<DaemonCommand>) -> Self {
        Self {
            app_state: AppState::Idle,
            icon_theme_path,
            last_transcript_preview: None,
            last_warning: None,
            command_sender,
        }
    }

    fn apply_event(&mut self, event: DaemonEvent) {
        match event {
            DaemonEvent::StateChanged(state) => self.app_state = state,
            DaemonEvent::TranscriptReady(transcript) => {
                self.last_transcript_preview = Some(preview_line(&transcript));
                self.last_warning = None;
            }
            DaemonEvent::Warning(message) => self.last_warning = Some(message),
        }
    }

    fn state_label(&self) -> &'static str {
        match self.app_state {
            AppState::Idle => "Ready",
            AppState::Recording => "Recording",
            AppState::Transcribing => "Transcribing",
        }
    }

    fn toggle_label(&self) -> &'static str {
        match self.app_state {
            AppState::Idle => "Start Recording",
            AppState::Recording => "Stop Recording",
            AppState::Transcribing => "Transcribing...",
        }
    }

    fn status_icon_name(&self) -> &'static str {
        match self.app_state {
            AppState::Idle => "onescribe-symbolic",
            AppState::Recording => "onescribe-recording-symbolic",
            AppState::Transcribing => "onescribe-transcribing-symbolic",
        }
    }

    fn tooltip_description(&self) -> String {
        let mut lines = vec![format!("Status: {}", self.state_label())];

        if let Some(warning) = &self.last_warning {
            lines.push(format!("Warning: {}", warning));
        } else if let Some(transcript) = &self.last_transcript_preview {
            lines.push(format!("Last transcript: {}", transcript));
        } else {
            lines.push("Hotkey: Alt+Space".to_string());
        }

        lines.join("\n")
    }
}

impl Tray for OneScribeTray {
    fn id(&self) -> String {
        HOST_APP_ID.to_string()
    }

    fn title(&self) -> String {
        format!("OneScribe ({})", self.state_label())
    }

    fn status(&self) -> Status {
        match self.app_state {
            AppState::Recording => Status::NeedsAttention,
            AppState::Idle | AppState::Transcribing => Status::Active,
        }
    }

    fn icon_theme_path(&self) -> String {
        self.icon_theme_path.clone()
    }

    fn icon_name(&self) -> String {
        self.status_icon_name().to_string()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "OneScribe".to_string(),
            description: self.tooltip_description(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        if self.app_state != AppState::Transcribing {
            eprintln!("command: toggle recording");
            let _ = self.command_sender.send(DaemonCommand::ToggleRecording);
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let toggle_sender = self.command_sender.clone();
        let quit_sender = self.command_sender.clone();

        vec![
            StandardItem {
                label: self.toggle_label().to_string(),
                enabled: self.app_state != AppState::Transcribing,
                activate: Box::new(move |_tray| {
                    eprintln!("command: toggle recording");
                    let _ = toggle_sender.send(DaemonCommand::ToggleRecording);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("Status: {}", self.state_label()),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Hotkey: Alt+Space".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self
                    .last_warning
                    .as_ref()
                    .map(|warning| format!("Warning: {}", preview_line(warning)))
                    .or_else(|| {
                        self.last_transcript_preview
                            .as_ref()
                            .map(|transcript| format!("Last: {}", transcript))
                    })
                    .unwrap_or_else(|| "Last: waiting for first transcript".to_string()),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(move |_tray| {
                    eprintln!("command: quit");
                    let _ = quit_sender.send(DaemonCommand::Shutdown);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn preview_line(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = normalized.chars().take(72).collect::<String>();
    if normalized.chars().count() > 72 {
        preview.push_str("...");
    }
    preview
}

fn echo_event_to_terminal(event: &DaemonEvent) {
    match event {
        DaemonEvent::StateChanged(AppState::Idle) => eprintln!("state: idle"),
        DaemonEvent::StateChanged(AppState::Recording) => eprintln!("state: recording"),
        DaemonEvent::StateChanged(AppState::Transcribing) => eprintln!("state: transcribing"),
        DaemonEvent::TranscriptReady(_) => {}
        DaemonEvent::Warning(message) => eprintln!("warning: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_line_compacts_whitespace_and_truncates() {
        let preview =
            preview_line("hello   world\nfrom    OneScribe and a much longer transcript");
        assert_eq!(
            preview,
            "hello world from OneScribe and a much longer transcript"
        );
    }

    #[test]
    fn tray_uses_recording_icon_for_recording_state() {
        let (command_sender, _command_receiver) = mpsc::unbounded_channel();
        let mut tray = OneScribeTray::new("/tmp/icons".to_string(), command_sender);
        tray.apply_event(DaemonEvent::StateChanged(AppState::Recording));

        assert_eq!(tray.status_icon_name(), "onescribe-recording-symbolic");
        assert_eq!(tray.state_label(), "Recording");
    }

    #[test]
    fn transcript_ready_events_are_terminal_visible() {
        echo_event_to_terminal(&DaemonEvent::TranscriptReady("hello world".to_string()));
    }
}
