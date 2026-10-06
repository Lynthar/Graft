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

use crate::site::Site;
use crate::torrent::MAX_TORRENT_BYTES;
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

/// Fill a site's download pattern. A placeholder whose value is not stored is an error,
/// so a half-filled URL is never requested.
pub(crate) fn fill_pattern(site: &Site, torrent_id: &str) -> Result<String> {
    let mut path = site.download_pattern.replace("{id}", torrent_id);
    if path.contains("{passkey}") {
        path = path.replace("{passkey}", site.passkey.as_deref().ok_or(TemplateError::MissingPasskey)?);
    }
    if path.contains("{authkey}") {
        path = path.replace("{authkey}", site.authkey.as_deref().ok_or(TemplateError::MissingAuthkey)?);
    }
    Ok(format!("{}{}", site.base_url, path))
}

/// GET a `.torrent` with the site's cookie. Login pages and bodies over
/// [`MAX_TORRENT_BYTES`] are errors; the bytes are otherwise unchecked.
pub(crate) async fn fetch_torrent(http: &reqwest::Client, site: &Site, torrent_id: &str) -> Result<Vec<u8>> {
    let url = fill_pattern(site, torrent_id)?;
    let mut request = http.get(&url).header("User-Agent", concat!("Graft/", env!("CARGO_PKG_VERSION")));
    if let Some(ref cookie) = site.cookie {
        request = request.header("Cookie", cookie);
    }
    let mut response = request.send().await?;
    if !response.status().is_success() {
        return Err(TemplateError::DownloadFailed(format!("HTTP {}", response.status())));
    }
    let is_html = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));
    if is_html {
        return Err(TemplateError::InvalidResponse(
            "the site answered with a web page instead of a torrent (login expired or no access?)".into(),
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_TORRENT_BYTES {
            return Err(TemplateError::InvalidResponse("the torrent file is too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Site template trait
///
/// Defines the interface for interacting with PT sites
#[async_trait]
pub trait SiteTemplate: Send + Sync {
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

    fn site(template_type: TemplateType, base_url: &str, download_pattern: &str) -> Site {
        Site {
            id: "test".to_string(),
            name: "Test".to_string(),
            base_url: base_url.to_string(),
            template_type,
            download_pattern: download_pattern.to_string(),
            passkey: Some("SECRET_PASSKEY".to_string()),
            cookie: None,
            authkey: None,
            enabled: true,
            rate_limit_rpm: 10,
            daily_limit: 20,
            builtin: false,
            domains: Vec::new(),
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

        assert!(matches!(fill_pattern(&config, "1"), Err(TemplateError::MissingAuthkey)));
    }
}
