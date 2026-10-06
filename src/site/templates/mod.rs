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

    #[error("Missing authkey")]
    MissingAuthkey,

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("HTTP error: {0}")]
    HttpError(reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

impl From<reqwest::Error> for TemplateError {
    // Download URLs carry the passkey; reqwest puts the URL into its message.
    fn from(err: reqwest::Error) -> Self {
        Self::HttpError(err.without_url())
    }
}

pub type Result<T> = std::result::Result<T, TemplateError>;

/// Site template trait
///
/// Defines the interface for interacting with PT sites
#[async_trait]
pub trait SiteTemplate: Send + Sync {
    /// Build download URL for a torrent
    fn build_download_url(&self, torrent_id: &str) -> Result<String>;

    /// Download a torrent file
    async fn download_torrent(
        &self,
        http_client: &reqwest::Client,
        torrent_id: &str,
    ) -> Result<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::SiteConfig;

    fn site(template_type: TemplateType, base_url: &str, download_pattern: &str) -> SiteConfig {
        SiteConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            base_url: base_url.to_string(),
            template_type,
            tracker_domains: Vec::new(),
            download_pattern: download_pattern.to_string(),
            passkey: Some("SECRET_PASSKEY".to_string()),
            cookie: None,
            enabled: true,
            rate_limit_rpm: None,
        }
    }

    #[tokio::test]
    async fn failed_download_does_not_expose_passkey() {
        // Nothing listens on loopback port 1, so the request fails inside reqwest.
        let config = site(
            TemplateType::NexusPHP,
            "http://127.0.0.1:1",
            "/download.php?id={id}&passkey={passkey}",
        );
        let err = config
            .create_template()
            .download_torrent(&reqwest::Client::new(), "123")
            .await
            .unwrap_err();

        assert!(matches!(err, TemplateError::HttpError(_)));
        assert!(!err.to_string().contains("SECRET_PASSKEY"), "{err}");
    }

    #[test]
    fn gazelle_url_needing_authkey_is_refused() {
        let config = site(
            TemplateType::Gazelle,
            "https://example.invalid",
            "/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}",
        );

        assert!(matches!(
            config.create_template().build_download_url("1"),
            Err(TemplateError::MissingAuthkey)
        ));
    }
}
