use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Local};

use crate::{journal_file_name, render_journal_entry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalStamp {
    pub file_date: String,
    pub header_date: String,
    pub time_stamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardCopyMethod {
    WlCopy,
    Xclip,
    Xsel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionSoundMethod {
    PwPlayFile,
}
const COMPLETION_SOUND_RELATIVE_PATH: &str = "assets/completion.oga";
const PANEL_ICON_IDLE_RELATIVE_PATH: &str =
    "assets/icons/hicolor/scalable/status/onescribe-symbolic.svg";
const PANEL_ICON_RECORDING_RELATIVE_PATH: &str =
    "assets/icons/hicolor/scalable/status/onescribe-recording-symbolic.svg";
const PANEL_ICON_TRANSCRIBING_RELATIVE_PATH: &str =
    "assets/icons/hicolor/scalable/status/onescribe-transcribing-symbolic.svg";
const APP_ICON_128_RELATIVE_PATH: &str =
    "../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_128x128.png";
const APP_ICON_256_RELATIVE_PATH: &str =
    "../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_256x256.png";
const APP_ICON_512_RELATIVE_PATH: &str =
    "../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_512x512.png";

#[derive(Debug)]
pub enum ClipboardCopyError {
    NoSupportedCommand,
    Io(io::Error),
    CommandFailed {
        command: &'static str,
        status_code: Option<i32>,
        stderr: String,
    },
}

impl std::fmt::Display for ClipboardCopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSupportedCommand => {
                write!(
                    f,
                    "no supported clipboard command found (tried wl-copy, xclip, xsel)"
                )
            }
            Self::Io(error) => write!(f, "{error}"),
            Self::CommandFailed {
                command,
                status_code,
                stderr,
            } => {
                write!(
                    f,
                    "{command} failed with status {:?}: {}",
                    status_code,
                    stderr.trim()
                )
            }
        }
    }
}

impl std::error::Error for ClipboardCopyError {}

#[derive(Debug)]
pub enum CompletionSoundError {
    MissingBundledSound(PathBuf),
    Io(io::Error),
    CommandFailed {
        command: &'static str,
        status_code: Option<i32>,
        stderr: String,
    },
}

impl std::fmt::Display for CompletionSoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBundledSound(path) => {
                write!(
                    f,
                    "bundled completion sound is missing at {}",
                    path.display()
                )
            }
            Self::Io(error) => write!(f, "{error}"),
            Self::CommandFailed {
                command,
                status_code,
                stderr,
            } => {
                write!(
                    f,
                    "{command} failed with status {:?}: {}",
                    status_code,
                    stderr.trim()
                )
            }
        }
    }
}

impl std::error::Error for CompletionSoundError {}

pub fn resolve_home_dir(environment: &[(impl AsRef<str>, impl AsRef<str>)]) -> Option<PathBuf> {
    env_value(environment, "HOME").map(PathBuf::from)
}

pub fn resolve_journal_dir(
    environment: &[(impl AsRef<str>, impl AsRef<str>)],
    home_dir: &Path,
) -> PathBuf {
    let documents_dir = env_value(environment, "XDG_DOCUMENTS_DIR")
        .map(|value| expand_home(&value, home_dir))
        .unwrap_or_else(|| home_dir.join("Documents"));

    documents_dir.join("OneScribe")
}

pub fn load_config_json(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn journal_stamp(now: DateTime<Local>) -> JournalStamp {
    JournalStamp {
        file_date: now.format("%Y-%m-%d").to_string(),
        header_date: now.format("%B %-d, %Y").to_string(),
        time_stamp: now.format("%-I:%M %p").to_string(),
    }
}

pub fn append_transcript_to_journal(
    journal_dir: &Path,
    stamp: &JournalStamp,
    text: &str,
) -> io::Result<PathBuf> {
    fs::create_dir_all(journal_dir)?;

    let journal_path = journal_dir.join(journal_file_name(&stamp.file_date));
    let existing = load_config_json(&journal_path)?;
    let rendered = render_journal_entry(
        existing.as_deref(),
        &stamp.header_date,
        &stamp.time_stamp,
        text,
    );

    fs::write(&journal_path, rendered)?;
    Ok(journal_path)
}

pub fn copy_to_clipboard(text: &str) -> Result<ClipboardCopyMethod, ClipboardCopyError> {
    let candidates = [
        (
            ClipboardCopyMethod::WlCopy,
            "wl-copy",
            vec!["--type", "text/plain"],
        ),
        (
            ClipboardCopyMethod::Xclip,
            "xclip",
            vec!["-selection", "clipboard"],
        ),
        (
            ClipboardCopyMethod::Xsel,
            "xsel",
            vec!["--clipboard", "--input"],
        ),
    ];

    let mut saw_supported_command = false;

    for (method, command, args) in candidates {
        match run_clipboard_command(command, &args, text) {
            Ok(()) => return Ok(method),
            Err(ClipboardCopyError::NoSupportedCommand) => continue,
            Err(error) => {
                saw_supported_command = true;
                if !matches!(error, ClipboardCopyError::NoSupportedCommand) {
                    return Err(error);
                }
            }
        }
    }

    if saw_supported_command {
        Err(ClipboardCopyError::NoSupportedCommand)
    } else {
        Err(ClipboardCopyError::NoSupportedCommand)
    }
}

pub fn play_completion_sound() -> Result<CompletionSoundMethod, CompletionSoundError> {
    let sound_path = bundled_completion_sound_path();
    if !sound_path.exists() {
        return Err(CompletionSoundError::MissingBundledSound(sound_path));
    }

    run_sound_file_command(
        "pw-play",
        &["--media-role", "Notification", "--volume", "0.65"],
        &sound_path,
    )?;
    Ok(CompletionSoundMethod::PwPlayFile)
}

pub fn install_linux_icon_assets(home_dir: &Path) -> io::Result<PathBuf> {
    let icon_root = home_dir.join(".local/share/icons");

    install_icon_asset(
        &bundled_asset_path(APP_ICON_128_RELATIVE_PATH),
        &icon_root.join("hicolor/128x128/apps/onescribe.png"),
    )?;
    install_icon_asset(
        &bundled_asset_path(APP_ICON_256_RELATIVE_PATH),
        &icon_root.join("hicolor/256x256/apps/onescribe.png"),
    )?;
    install_icon_asset(
        &bundled_asset_path(APP_ICON_512_RELATIVE_PATH),
        &icon_root.join("hicolor/512x512/apps/onescribe.png"),
    )?;
    install_icon_asset(
        &bundled_asset_path(PANEL_ICON_IDLE_RELATIVE_PATH),
        &icon_root.join("hicolor/scalable/status/onescribe-symbolic.svg"),
    )?;
    install_icon_asset(
        &bundled_asset_path(PANEL_ICON_RECORDING_RELATIVE_PATH),
        &icon_root.join("hicolor/scalable/status/onescribe-recording-symbolic.svg"),
    )?;
    install_icon_asset(
        &bundled_asset_path(PANEL_ICON_TRANSCRIBING_RELATIVE_PATH),
        &icon_root.join("hicolor/scalable/status/onescribe-transcribing-symbolic.svg"),
    )?;

    invalidate_icon_theme_cache(&icon_root)?;

    Ok(icon_root)
}

/// Drops the `hicolor` icon-theme cache so GTK/St re-index the directory.
///
/// GTK trusts `icon-theme.cache` when it is present and will not notice icons
/// that were added or renamed since it was generated, which shows up as a
/// generic fallback glyph in the tray. Removing it (GTK rebuilds an in-memory
/// index on demand) keeps freshly installed icons visible without depending on
/// `gtk-update-icon-cache` being available.
fn invalidate_icon_theme_cache(icon_root: &Path) -> io::Result<()> {
    match fs::remove_file(icon_root.join("hicolor/icon-theme.cache")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn guess_mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("m4a") => "audio/m4a",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("webm") => "audio/webm",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    }
}

pub fn current_environment() -> Vec<(String, String)> {
    env::vars().collect()
}

fn env_value(environment: &[(impl AsRef<str>, impl AsRef<str>)], key: &str) -> Option<String> {
    environment
        .iter()
        .find(|(candidate, _)| candidate.as_ref() == key)
        .map(|(_, value)| value.as_ref().to_string())
}

fn expand_home(path: &str, home_dir: &Path) -> PathBuf {
    if path == "~" {
        return home_dir.to_path_buf();
    }

    if let Some(stripped) = path.strip_prefix("~/") {
        return home_dir.join(stripped);
    }

    PathBuf::from(path)
}

fn run_clipboard_command(
    command: &'static str,
    args: &[&str],
    text: &str,
) -> Result<(), ClipboardCopyError> {
    let mut child = match Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ClipboardCopyError::NoSupportedCommand);
        }
        Err(error) => return Err(ClipboardCopyError::Io(error)),
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(ClipboardCopyError::Io)?;
        drop(stdin);
    }

    let status = child.wait().map_err(ClipboardCopyError::Io)?;
    if status.success() {
        return Ok(());
    }

    Err(ClipboardCopyError::CommandFailed {
        command,
        status_code: status.code(),
        stderr: String::new(),
    })
}

fn run_sound_file_command(
    command: &'static str,
    args: &[&str],
    sound_path: &Path,
) -> Result<(), CompletionSoundError> {
    let output = match Command::new(command).args(args).arg(sound_path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(CompletionSoundError::Io(error));
        }
        Err(error) => return Err(CompletionSoundError::Io(error)),
    };

    if output.status.success() {
        return Ok(());
    }

    Err(CompletionSoundError::CommandFailed {
        command,
        status_code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn bundled_completion_sound_path() -> PathBuf {
    bundled_asset_path(COMPLETION_SOUND_RELATIVE_PATH)
}

fn bundled_asset_path(relative_path: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        // In dev builds, use CARGO_MANIFEST_DIR for live-editing assets
        Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
    } else {
        // In release builds, resolve relative to the binary's directory
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        exe_dir.join(relative_path)
    }
}

fn install_icon_asset(source_path: &Path, destination_path: &Path) -> io::Result<()> {
    if !source_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing icon asset at {}", source_path.display()),
        ));
    }

    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let should_copy = match fs::read(destination_path) {
        Ok(existing) => existing != fs::read(source_path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    };

    if should_copy {
        fs::copy(source_path, destination_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn journal_dir_uses_xdg_documents_when_present() {
        let environment = [("XDG_DOCUMENTS_DIR", "~/Files/Documents")];
        let path = resolve_journal_dir(&environment, Path::new("/home/tester"));

        assert_eq!(
            path,
            PathBuf::from("/home/tester/Files/Documents/OneScribe")
        );
    }

    #[test]
    fn journal_dir_defaults_to_home_documents() {
        let environment: [(&str, &str); 0] = [];
        let path = resolve_journal_dir(&environment, Path::new("/home/tester"));

        assert_eq!(path, PathBuf::from("/home/tester/Documents/OneScribe"));
    }

    #[test]
    fn append_transcript_creates_new_markdown_file() {
        let temp_dir = unique_temp_dir();
        let stamp = JournalStamp {
            file_date: "2026-03-07".to_string(),
            header_date: "March 7, 2026".to_string(),
            time_stamp: "9:41 AM".to_string(),
        };

        let journal_path = append_transcript_to_journal(&temp_dir, &stamp, "First entry").unwrap();
        let contents = fs::read_to_string(journal_path).unwrap();

        assert_eq!(
            contents,
            "# Transcriptions - March 7, 2026\n\n## 9:41 AM\n\nFirst entry\n\n---\n\n"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn append_transcript_appends_to_existing_file() {
        let temp_dir = unique_temp_dir();
        let initial_stamp = JournalStamp {
            file_date: "2026-03-07".to_string(),
            header_date: "March 7, 2026".to_string(),
            time_stamp: "9:41 AM".to_string(),
        };
        let second_stamp = JournalStamp {
            file_date: "2026-03-07".to_string(),
            header_date: "March 7, 2026".to_string(),
            time_stamp: "9:45 AM".to_string(),
        };

        let journal_path =
            append_transcript_to_journal(&temp_dir, &initial_stamp, "First entry").unwrap();
        append_transcript_to_journal(&temp_dir, &second_stamp, "Second entry").unwrap();

        let contents = fs::read_to_string(journal_path).unwrap();
        assert_eq!(
            contents,
            "# Transcriptions - March 7, 2026\n\n## 9:41 AM\n\nFirst entry\n\n---\n\n## 9:45 AM\n\nSecond entry\n\n---\n\n"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn bundled_completion_sound_path_points_to_asset_file() {
        let path = bundled_completion_sound_path();
        assert!(path.ends_with("assets/completion.oga"));
    }

    #[test]
    fn bundled_completion_sound_path_is_inside_manifest_dir() {
        let path = bundled_completion_sound_path();
        #[cfg(debug_assertions)]
        {
            let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
            assert!(path.starts_with(manifest_dir));
        }
        #[cfg(not(debug_assertions))]
        {
            let exe_path = std::env::current_exe().unwrap();
            let exe_dir = exe_path.parent().unwrap();
            assert!(path.starts_with(exe_dir));
        }
    }

    #[test]
    fn install_linux_icon_assets_returns_icon_root() {
        let temp_home = unique_temp_dir();
        let icon_root = install_linux_icon_assets(&temp_home).unwrap();

        assert_eq!(icon_root, temp_home.join(".local/share/icons"));
        assert!(
            icon_root
                .join("hicolor/scalable/status/onescribe-symbolic.svg")
                .exists()
        );
        assert!(
            icon_root
                .join("hicolor/512x512/apps/onescribe.png")
                .exists()
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn install_linux_icon_assets_invalidates_stale_icon_theme_cache() {
        let temp_home = unique_temp_dir();
        let cache = temp_home.join(".local/share/icons/hicolor/icon-theme.cache");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, b"stale index").unwrap();

        install_linux_icon_assets(&temp_home).unwrap();

        assert!(
            !cache.exists(),
            "a stale icon-theme cache must be dropped so GTK re-indexes"
        );
        let _ = fs::remove_dir_all(&temp_home);
    }

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("onescribe-linux-test-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
