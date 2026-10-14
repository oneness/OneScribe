use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionBackendChoice {
    Groq,
    Local,
}

/// All settings the app reads, resolved once at startup. Each field comes
/// from its environment variable when set (trimmed, non-empty), else the
/// config-file key, else the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// ONESCRIBE_BACKEND / "backend": "local" or "groq". Local is the
    /// default — Groq runs only when explicitly selected.
    pub backend: TranscriptionBackendChoice,
    /// ONESCRIBE_MODEL_PATH / "model_path", defaulting to the bundled model
    /// under ~/.local/share. `~` is expanded.
    pub model_path: PathBuf,
    /// ONESCRIBE_LANGUAGE / "language" (e.g. "en-US"); None = autodetect.
    pub language: Option<String>,
    /// GROQ_API_KEY / "groq_api_key" (or legacy "GROQ_API_KEY" file key).
    pub groq_api_key: Option<String>,
}

impl Config {
    pub fn resolve(
        environment: &[(impl AsRef<str>, impl AsRef<str>)],
        config_json: Option<&str>,
        home_dir: &Path,
    ) -> Self {
        let parsed: Option<serde_json::Value> =
            config_json.and_then(|json| serde_json::from_str(json).ok());
        let file_field = |key: &str| -> Option<String> {
            parsed
                .as_ref()
                .and_then(|value| value.get(key))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let setting =
            |env_key: &str, file_key: &str| non_empty_env(environment, env_key).or_else(|| file_field(file_key));

        let backend = match setting("ONESCRIBE_BACKEND", "backend").as_deref() {
            None => TranscriptionBackendChoice::Local,
            Some(value) if value.eq_ignore_ascii_case("groq") => TranscriptionBackendChoice::Groq,
            Some(value) if value.eq_ignore_ascii_case("local") => TranscriptionBackendChoice::Local,
            Some(other) => {
                eprintln!("warning: unrecognized backend {other:?}; using local");
                TranscriptionBackendChoice::Local
            }
        };

        let model_path = setting("ONESCRIBE_MODEL_PATH", "model_path")
            .map(|path| expand_home(&path, home_dir))
            .unwrap_or_else(|| {
                home_dir
                    .join(".local/share/onescribe/models")
                    .join(crate::DEFAULT_LOCAL_MODEL_FILE)
            });

        let language = setting("ONESCRIBE_LANGUAGE", "language");

        let groq_api_key =
            setting("GROQ_API_KEY", "groq_api_key").or_else(|| file_field("GROQ_API_KEY"));

        Self {
            backend,
            model_path,
            language,
            groq_api_key,
        }
    }
}

pub fn resolve_config_path(
    environment: &[(impl AsRef<str>, impl AsRef<str>)],
    home_dir: &Path,
) -> PathBuf {
    if let Some(override_path) = env_value(environment, "ONESCRIBE_CONFIG_PATH") {
        return expand_home(&override_path, home_dir);
    }

    let base_dir = env_value(environment, "XDG_CONFIG_HOME")
        .map(|value| expand_home(&value, home_dir))
        .unwrap_or_else(|| home_dir.join(".config"));

    base_dir.join("onescribe").join("config.json")
}

fn non_empty_env(
    environment: &[(impl AsRef<str>, impl AsRef<str>)],
    key: &str,
) -> Option<String> {
    env_value(environment, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/tester";
    const NO_ENV: [(&str, &str); 0] = [];

    fn resolve(
        environment: &[(&str, &str)],
        config_json: Option<&str>,
    ) -> Config {
        Config::resolve(environment, config_json, Path::new(HOME))
    }

    #[test]
    fn environment_api_key_wins_over_config_file() {
        let environment = [
            ("GROQ_API_KEY", "env-key"),
            ("XDG_CONFIG_HOME", "/tmp/config"),
        ];
        let config = resolve(&environment, Some(r#"{"groq_api_key":"file-key"}"#));

        assert_eq!(config.groq_api_key.as_deref(), Some("env-key"));
    }

    #[test]
    fn config_accepts_both_supported_api_key_names() {
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"groq_api_key":"abc"}"#))
                .groq_api_key
                .as_deref(),
            Some("abc")
        );
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"GROQ_API_KEY":"xyz"}"#))
                .groq_api_key
                .as_deref(),
            Some("xyz")
        );
    }

    #[test]
    fn backend_defaults_to_local_even_with_api_key_present() {
        assert_eq!(
            resolve(&NO_ENV, None).backend,
            TranscriptionBackendChoice::Local
        );
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"groq_api_key":"abc"}"#)).backend,
            TranscriptionBackendChoice::Local
        );
    }

    #[test]
    fn backend_groq_only_when_explicitly_selected() {
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"backend":"groq"}"#)).backend,
            TranscriptionBackendChoice::Groq
        );
        assert_eq!(
            resolve(
                &[("ONESCRIBE_BACKEND", "groq")],
                Some(r#"{"backend":"local"}"#)
            )
            .backend,
            TranscriptionBackendChoice::Groq
        );
        assert_eq!(
            resolve(&[("ONESCRIBE_BACKEND", "Local")], None).backend,
            TranscriptionBackendChoice::Local
        );
    }

    #[test]
    fn unrecognized_backend_falls_back_to_local() {
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"backend":"grok"}"#)).backend,
            TranscriptionBackendChoice::Local
        );
    }

    #[test]
    fn empty_env_overrides_are_ignored() {
        let environment = [("ONESCRIBE_BACKEND", ""), ("ONESCRIBE_MODEL_PATH", " ")];
        let config = resolve(
            &environment,
            Some(r#"{"backend":"local","model_path":"~/models/x.gguf"}"#),
        );

        assert_eq!(config.backend, TranscriptionBackendChoice::Local);
        assert_eq!(config.model_path, PathBuf::from("/home/tester/models/x.gguf"));
    }

    #[test]
    fn model_path_resolution_order() {
        assert_eq!(
            resolve(&NO_ENV, None).model_path,
            PathBuf::from("/home/tester/.local/share/onescribe/models")
                .join(crate::DEFAULT_LOCAL_MODEL_FILE)
        );
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"model_path":"~/models/x.gguf"}"#)).model_path,
            PathBuf::from("/home/tester/models/x.gguf")
        );
        assert_eq!(
            resolve(
                &[("ONESCRIBE_MODEL_PATH", "/opt/models/y.gguf")],
                Some(r#"{"model_path":"~/models/x.gguf"}"#)
            )
            .model_path,
            PathBuf::from("/opt/models/y.gguf")
        );
    }

    #[test]
    fn language_resolution_env_then_config_then_autodetect() {
        assert_eq!(resolve(&NO_ENV, None).language, None);
        assert_eq!(
            resolve(&NO_ENV, Some(r#"{"language":"en-US"}"#))
                .language
                .as_deref(),
            Some("en-US")
        );
        assert_eq!(
            resolve(
                &[("ONESCRIBE_LANGUAGE", "de-DE")],
                Some(r#"{"language":"en-US"}"#)
            )
            .language
            .as_deref(),
            Some("de-DE")
        );
    }

    #[test]
    fn override_config_path_wins() {
        let environment = [("ONESCRIBE_CONFIG_PATH", "~/custom/config.json")];
        let path = resolve_config_path(&environment, Path::new(HOME));

        assert_eq!(path, PathBuf::from("/home/tester/custom/config.json"));
    }

    #[test]
    fn xdg_config_path_is_used_when_present() {
        let environment = [("XDG_CONFIG_HOME", "/home/tester/.config-alt")];
        let path = resolve_config_path(&environment, Path::new(HOME));

        assert_eq!(
            path,
            PathBuf::from("/home/tester/.config-alt/onescribe/config.json")
        );
    }

    #[test]
    fn default_config_path_falls_back_to_dot_config() {
        let path = resolve_config_path(&NO_ENV, Path::new(HOME));

        assert_eq!(
            path,
            PathBuf::from("/home/tester/.config/onescribe/config.json")
        );
    }
}
