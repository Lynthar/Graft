//! Gazelle site template
//!
//! Gazelle is a PT framework commonly used by music trackers like Redacted, Orpheus.

use async_trait::async_trait;

use super::{Result, SiteTemplate, TemplateError};
use crate::site::SiteConfig;

pub struct GazelleTemplate {
    config: SiteConfig,
}

impl GazelleTemplate {
    pub fn new(config: SiteConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SiteTemplate for GazelleTemplate {
    fn build_download_url(&self, torrent_id: &str) -> Result<String> {
        // No site field stores the user's authkey yet, so such a URL can't be built.
        if self.config.download_pattern.contains("{authkey}") {
            return Err(TemplateError::MissingAuthkey);
        }

        let passkey = self.config.passkey.as_ref()
            .ok_or(TemplateError::MissingPasskey)?;

        let url = self.config.download_pattern
            .replace("{id}", torrent_id)
            .replace("{passkey}", passkey);

        Ok(format!("{}{}", self.config.base_url, url))
    }

    async fn download_torrent(
        &self,
        http_client: &reqwest::Client,
        torrent_id: &str,
    ) -> Result<Vec<u8>> {
        let url = self.build_download_url(torrent_id)?;

        let mut request = http_client.get(&url);

        // Gazelle sites typically require cookie authentication
        if let Some(ref cookie) = self.config.cookie {
            request = request.header("Cookie", cookie);
        }

        let response = request
            .header("User-Agent", "Graft/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TemplateError::DownloadFailed(format!(
                "HTTP {}: {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        let bytes = response.bytes().await?;

        // Verify it's a valid torrent file
        if bytes.first() != Some(&b'd') {
            // Check if it's a JSON error response
            if let Ok(text) = std::str::from_utf8(&bytes) {
                if text.contains("error") || text.contains("failure") {
                    return Err(TemplateError::InvalidResponse(text.to_string()));
                }
            }
            return Err(TemplateError::InvalidResponse(
                "Invalid torrent file format".to_string()
            ));
        }

        Ok(bytes.to_vec())
    }
}
