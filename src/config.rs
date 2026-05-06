use crate::types::CliType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct DefaultSession {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) cli_type: CliType,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) default_sessions: Vec<DefaultSession>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_sessions: vec![
                DefaultSession {
                    name: "claude skip-perm".to_string(),
                    cli_type: CliType::ClaudeDangerous,
                },
                DefaultSession {
                    name: "claude".to_string(),
                    cli_type: CliType::Claude,
                },
                DefaultSession {
                    name: "console".to_string(),
                    cli_type: CliType::Console,
                },
            ],
        }
    }
}

/// Standard config directory for the current OS.
///
/// - macOS:   `$HOME/Library/Application Support/neimar`
/// - Windows: `%APPDATA%\neimar`
/// - Linux:   `$XDG_CONFIG_HOME/neimar` or `$HOME/.config/neimar`
pub(crate) fn config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/neimar"))
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("neimar"))
    } else {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            Some(PathBuf::from(xdg).join("neimar"))
        } else {
            let home = std::env::var_os("HOME")?;
            Some(PathBuf::from(home).join(".config/neimar"))
        }
    }
}

pub(crate) fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.yaml"))
}

impl Config {
    /// Load the config from the standard location, or write a default if missing.
    /// On parse errors the existing file is left untouched and defaults are used.
    pub(crate) fn load_or_create() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };

        if path.exists() {
            return std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_yml::from_str(&s).ok())
                .unwrap_or_default();
        }

        let cfg = Self::default();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(yaml) = serde_yml::to_string(&cfg) {
            let _ = std::fs::write(&path, yaml);
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_to_expected_yaml() {
        let cfg = Config::default();
        let yaml = serde_yml::to_string(&cfg).unwrap();
        assert!(yaml.contains("name: claude skip-perm"));
        assert!(yaml.contains("type: claude-dangerous"));
        assert!(yaml.contains("type: claude"));
        assert!(yaml.contains("type: console"));

        let parsed: Config = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(parsed.default_sessions.len(), 3);
        assert_eq!(parsed.default_sessions[0].name, "claude skip-perm");
        assert!(matches!(
            parsed.default_sessions[0].cli_type,
            CliType::ClaudeDangerous
        ));
    }

    #[test]
    fn empty_default_sessions_parses() {
        let yaml = "default_sessions: []\n";
        let cfg: Config = serde_yml::from_str(yaml).unwrap();
        assert!(cfg.default_sessions.is_empty());
    }

    #[test]
    fn missing_default_sessions_field_uses_default() {
        let cfg: Config = serde_yml::from_str("{}").unwrap();
        assert!(cfg.default_sessions.is_empty());
    }
}
