//! Configuration management module

use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Application settings. Unknown keys are errors: a setting that is accepted must take effect.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerSettings,

    #[serde(default)]
    pub database: DatabaseSettings,

    #[serde(skip)]
    config_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    /// Required when `host` is not a loopback address; enforced whenever it is set.
    #[serde(default)]
    pub password: Option<Password>,
}

/// The access password. `Debug` prints a placeholder so the value never reaches a log.
#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct Password(String);

impl Password {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

/// `localhost` or a loopback IP address; any other name may reach other machines.
pub fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSettings {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/graft.db")
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            password: None,
        }
    }
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

impl Settings {
    /// Load settings from environment and config file
    pub fn load() -> Result<Self> {
        // Load .env file if present
        let _ = dotenvy::dotenv();

        // Try to find config file
        let mut config_paths = vec![
            PathBuf::from("config.toml"),
            PathBuf::from("./data/config.toml"),
        ];
        if let Some(path) = dirs_config_path() {
            config_paths.push(path);
        }

        let mut settings = Settings::default();

        for path in config_paths.iter() {
            if path.exists() {
                settings = Self::load_from_file(path)?;
                settings.config_file = Some(path.clone());
                break;
            }
        }

        // Override with environment variables
        settings.apply_env_overrides()?;
        if settings.server.password.is_none() && !is_loopback(&settings.server.host) {
            anyhow::bail!(
                "Listening on {} needs a password: set GRAFT_PASSWORD (or server.password), \
                 or listen on 127.0.0.1",
                settings.server.host
            );
        }

        Ok(settings)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let settings: Settings = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;

        Ok(settings)
    }

    fn apply_env_overrides(&mut self) -> Result<()> {
        if let Ok(host) = std::env::var("GRAFT_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("GRAFT_PORT") {
            self.server.port = port
                .parse()
                .with_context(|| format!("GRAFT_PORT must be a port number, got {port:?}"))?;
        }
        if let Ok(path) = std::env::var("GRAFT_DATA_DIR") {
            self.database.path = PathBuf::from(path).join("graft.db");
        }
        if let Ok(path) = std::env::var("GRAFT_DB_PATH") {
            self.database.path = PathBuf::from(path);
        }
        if let Ok(password) = std::env::var("GRAFT_PASSWORD") {
            self.server.password = Some(Password(password));
        }
        // An empty password would let anyone in while looking like protection.
        if self.server.password.as_ref().is_some_and(|p| p.0.is_empty()) {
            self.server.password = None;
        }
        Ok(())
    }

    /// Get the path to the config file (if loaded from file)
    pub fn config_path(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }
}

/// Get platform-specific config directory
fn dirs_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
            .map(|p| p.join("graft/config.toml"))
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support/graft/config.toml"))
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("graft/config.toml"))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listens_on_loopback_by_default() {
        assert_eq!(Settings::default().server.host, "127.0.0.1");
    }

    #[test]
    fn only_loopback_names_count_as_loopback() {
        for host in ["127.0.0.1", "127.1.2.3", "::1", "localhost", "LOCALHOST"] {
            assert!(is_loopback(host), "{host}");
        }
        for host in ["0.0.0.0", "::", "192.168.1.10", "nas.local", "localhost.evil.example"] {
            assert!(!is_loopback(host), "{host}");
        }
    }

    #[test]
    fn the_password_never_shows_in_debug_output() {
        let settings: Settings = toml::from_str("[server]\npassword = \"hunter2\"\n").unwrap();
        assert_eq!(settings.server.password.as_ref().unwrap().expose(), "hunter2");
        assert!(!format!("{settings:?}").contains("hunter2"));
    }

    #[test]
    fn unknown_keys_are_rejected_not_ignored() {
        assert!(toml::from_str::<Settings>("[server]\nport = 3001\n").is_ok());
        assert!(toml::from_str::<Settings>("[server]\nprot = 3001\n").is_err());
        assert!(toml::from_str::<Settings>("[reseed]\ndefault_paused = true\n").is_err());
    }
}
