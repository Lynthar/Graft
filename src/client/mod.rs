//! BitTorrent client abstraction layer
//!
//! This module provides a unified interface for interacting with different
//! BitTorrent clients (qBittorrent, Transmission, etc.)

mod qbittorrent;
mod transmission;

pub use qbittorrent::QBittorrentClient;
pub use transmission::TransmissionClient;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Unified error type for client operations
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Torrent not found: {0}")]
    TorrentNotFound(String),

    #[error("Operation not supported")]
    NotSupported,
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// BitTorrent client types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    QBittorrent,
    Transmission,
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientType::QBittorrent => write!(f, "qbittorrent"),
            ClientType::Transmission => write!(f, "transmission"),
        }
    }
}

impl std::str::FromStr for ClientType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "qbittorrent" | "qb" => Ok(ClientType::QBittorrent),
            "transmission" | "tr" => Ok(ClientType::Transmission),
            _ => Err(format!("Unknown client type: {}", s)),
        }
    }
}

/// Torrent state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TorrentState {
    Downloading,
    Seeding,
    Paused,
    Checking,
    Error,
    Queued,
    Stalled,
    Unknown,
}

/// Information about a torrent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    pub name: String,
    pub size: u64,
    pub progress: f64,
    pub state: TorrentState,
    pub save_path: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub tracker: Option<String>,
    pub trackers: Vec<String>,
    pub added_on: Option<DateTime<Utc>>,
    pub files: Vec<TorrentFile>,
}

/// Information about a file in a torrent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub name: String,
    pub size: u64,
    pub progress: f64,
}

/// Options for adding a torrent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddTorrentOptions {
    pub save_path: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub paused: bool,
    pub skip_checking: bool,
}

/// Unified interface for BitTorrent clients
#[async_trait]
pub trait BitTorrentClient: Send + Sync {
    /// Get the client type
    fn client_type(&self) -> ClientType;

    /// Get the client ID
    fn client_id(&self) -> &str;

    /// Test the connection to the client
    async fn test_connection(&self) -> Result<bool>;

    /// Get all torrents
    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>>;

    /// Get a specific torrent by hash
    async fn get_torrent(&self, hash: &str) -> Result<Option<TorrentInfo>>;

    /// Get files for a specific torrent
    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>>;

    /// Get trackers for a specific torrent
    async fn get_torrent_trackers(&self, hash: &str) -> Result<Vec<String>>;

    /// Add a torrent from bytes
    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<String>;

    /// Remove a torrent
    async fn remove_torrent(&self, hash: &str, delete_files: bool) -> Result<()>;

    /// Pause a torrent
    async fn pause_torrent(&self, hash: &str) -> Result<()>;

    /// Resume a torrent
    async fn resume_torrent(&self, hash: &str) -> Result<()>;

    /// Force recheck a torrent
    async fn recheck_torrent(&self, hash: &str) -> Result<()>;
}

/// Client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub id: String,
    pub name: String,
    pub client_type: ClientType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_https: bool,
}

impl ClientConfig {
    /// Create a new client instance based on the configuration
    pub fn create_client(&self) -> Box<dyn BitTorrentClient> {
        match self.client_type {
            ClientType::QBittorrent => Box::new(QBittorrentClient::new(self.clone())),
            ClientType::Transmission => Box::new(TransmissionClient::new(self.clone())),
        }
    }

    /// Get the base URL for the client
    pub fn base_url(&self) -> String {
        let scheme = if self.use_https { "https" } else { "http" };
        format!("{}://{}:{}", scheme, self.host, self.port)
    }
}
