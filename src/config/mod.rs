//! Configuration management module

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerSettings,

    #[serde(default)]
    pub database: DatabaseSettings,

    #[serde(default)]
    pub reseed: ReseedSettings,

    #[serde(skip)]
    config_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSettings {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedSettings {
    /// Whether to add torrents in paused state
    #[serde(default)]
    pub default_paused: bool,

    /// Request interval in milliseconds to avoid rate limiting
    #[serde(default = "default_request_interval")]
    pub request_interval_ms: u64,

    /// Maximum number of torrents to process per run
    #[serde(default = "default_max_per_run")]
    pub max_per_run: usize,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/graft.db")
}

fn default_request_interval() -> u64 {
    500
}

fn default_max_per_run() -> usize {
    100
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
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

impl Default for ReseedSettings {
    fn default() -> Self {
        Self {
            default_paused: false,
            request_interval_ms: default_request_interval(),
            max_per_run: default_max_per_run(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ServerSettings::default(),
            database: DatabaseSettings::default(),
            reseed: ReseedSettings::default(),
            config_file: None,
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
        settings.apply_env_overrides();

        // Ensure data directory exists
        if let Some(parent) = settings.database.path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create data directory")?;
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

    fn apply_env_overrides(&mut self) {
        if let Ok(host) = std::env::var("GRAFT_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("GRAFT_PORT") {
            if let Ok(port) = port.parse() {
                self.server.port = port;
            }
        }
        if let Ok(path) = std::env::var("GRAFT_DATA_DIR") {
            self.database.path = PathBuf::from(path).join("graft.db");
        }
        if let Ok(path) = std::env::var("GRAFT_DB_PATH") {
            self.database.path = PathBuf::from(path);
        }
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

// Add toml dependency
fn _toml_parse_helper() {
    // This is a marker to remind us to add toml to Cargo.toml
}
