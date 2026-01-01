//! Gazelle site template
//!
//! Gazelle is a PT framework commonly used by music trackers like Redacted, Orpheus.

use async_trait::async_trait;

use super::{Result, SiteTemplate, TemplateError, TemplateType};
use crate::site::SiteConfig;

pub struct GazelleTemplate {
    config: SiteConfig,
    authkey: Option<String>,
}

impl GazelleTemplate {
    pub fn new(config: SiteConfig) -> Self {
        Self {
            config,
            authkey: None,
        }
    }

    pub fn with_authkey(mut self, authkey: String) -> Self {
        self.authkey = Some(authkey);
        self
    }
}

#[async_trait]
impl SiteTemplate for GazelleTemplate {
    fn config(&self) -> &SiteConfig {
        &self.config
    }

    fn template_type(&self) -> TemplateType {
        TemplateType::Gazelle
    }

    fn build_download_url(&self, torrent_id: &str) -> Result<String> {
        let passkey = self.config.passkey.as_ref()
            .ok_or(TemplateError::MissingPasskey)?;

        // Gazelle uses authkey + torrent_pass (passkey)
        // Format: /torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}
        let authkey = self.authkey.as_deref().unwrap_or("");

        let url = self.config.download_pattern
            .replace("{id}", torrent_id)
            .replace("{authkey}", authkey)
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
