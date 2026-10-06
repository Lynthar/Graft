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

/// Unified error type for client operations
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Torrent not found: {0}")]
    TorrentNotFound(String),

    #[error("Not supported: {0}")]
    Unsupported(&'static str),

    #[error("The client already has this torrent")]
    Duplicate,
}

impl From<reqwest::Error> for ClientError {
    // The cause (timeout, refused, reset) sits below reqwest's own message.
    fn from(err: reqwest::Error) -> Self {
        Self::RequestFailed(crate::error_chain(&err))
    }
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

/// A torrent as the downloader reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    /// Lowercase hex v1 info hash.
    pub hash: String,
    pub name: String,
    /// Total size of all files, selected or not.
    pub size: u64,
    pub progress: f64,
    pub save_path: String,
    /// The tracker currently in use; `None` when no tracker is working.
    pub tracker: Option<String>,
}

/// A file as it lies under the torrent's save path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    /// Relative path with `/` separators, including the root folder if there is one.
    pub name: String,
    pub size: u64,
}

/// How a torrent is added. There is deliberately no way to start it or skip the hash
/// check: a reseeded torrent must not touch existing data before the downloader has
/// verified it piece by piece.
#[derive(Debug, Clone)]
pub struct AddTorrentOptions {
    /// Used as-is: automatic management and content-layout rewrites are switched off.
    pub save_path: String,
    pub tags: Vec<String>,
}

/// Unified interface for BitTorrent clients
#[async_trait]
pub trait BitTorrentClient: Send + Sync {
    /// Test the connection to the client
    async fn test_connection(&self) -> Result<bool>;

    /// Get all torrents
    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>>;

    /// Hex SHA-1 of every piece, in order.
    ///
    /// # Errors
    /// `Unsupported` where the client cannot report piece hashes; an empty list means
    /// the torrent has no metadata yet.
    async fn get_piece_hashes(&self, hash: &str) -> Result<Vec<String>>;

    /// Files of a torrent as laid out on disk.
    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>>;

    /// Whether the client currently holds a torrent with this info hash.
    async fn has_torrent(&self, hash: &str) -> Result<bool>;

    /// Hand a torrent to the client, stopped and unchecked.
    ///
    /// # Errors
    /// `Duplicate` when the client reports it already has the torrent. Success only
    /// means the request was accepted; confirm with [`Self::has_torrent`].
    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<()>;
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
