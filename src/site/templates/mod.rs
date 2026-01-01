//! Site template implementations
//!
//! Templates define how to interact with different PT site frameworks
//! (NexusPHP, Unit3D, Gazelle, etc.)

mod nexusphp;
mod unit3d;
mod gazelle;

pub use nexusphp::NexusPHPTemplate;
pub use unit3d::Unit3DTemplate;
pub use gazelle::GazelleTemplate;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::SiteConfig;

/// Template type enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateType {
    NexusPHP,
    Unit3D,
    Gazelle,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateType::NexusPHP => write!(f, "nexusphp"),
            TemplateType::Unit3D => write!(f, "unit3d"),
            TemplateType::Gazelle => write!(f, "gazelle"),
        }
    }
}

impl std::str::FromStr for TemplateType {
    type Err = TemplateError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nexusphp" | "nexus" => Ok(TemplateType::NexusPHP),
            "unit3d" => Ok(TemplateType::Unit3D),
            "gazelle" => Ok(TemplateType::Gazelle),
            _ => Err(TemplateError::InvalidResponse(format!("Unknown template type: {}", s))),
        }
    }
}

/// Error type for template operations
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Missing passkey")]
    MissingPasskey,

    #[error("Missing cookie")]
    MissingCookie,

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, TemplateError>;

/// Site template trait
///
/// Defines the interface for interacting with PT sites
#[async_trait]
pub trait SiteTemplate: Send + Sync {
    /// Get site configuration
    fn config(&self) -> &SiteConfig;

    /// Get template type
    fn template_type(&self) -> TemplateType;

    /// Build download URL for a torrent
    fn build_download_url(&self, torrent_id: &str) -> Result<String>;

    /// Download a torrent file
    async fn download_torrent(
        &self,
        http_client: &reqwest::Client,
        torrent_id: &str,
    ) -> Result<Vec<u8>>;
}
