//! NexusPHP site template
//!
//! NexusPHP is the most common PT site framework, used by many Chinese PT sites.

use async_trait::async_trait;

use super::{Result, SiteTemplate, TemplateError, TemplateType};
use crate::site::SiteConfig;

pub struct NexusPHPTemplate {
    config: SiteConfig,
}

impl NexusPHPTemplate {
    pub fn new(config: SiteConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SiteTemplate for NexusPHPTemplate {
    fn config(&self) -> &SiteConfig {
        &self.config
    }

    fn template_type(&self) -> TemplateType {
        TemplateType::NexusPHP
    }

    fn build_download_url(&self, torrent_id: &str) -> Result<String> {
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

        // Add cookie if available
        if let Some(ref cookie) = self.config.cookie {
            request = request.header("Cookie", cookie);
        }

        let response = request
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TemplateError::DownloadFailed(format!(
                "HTTP {}: {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Check content type
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/html") {
            // Probably an error page
            let text = response.text().await?;
            if text.contains("login") || text.contains("登录") {
                return Err(TemplateError::MissingCookie);
            }
            return Err(TemplateError::InvalidResponse(
                "Received HTML instead of torrent file".to_string()
            ));
        }

        let bytes = response.bytes().await?;

        // Verify it's a valid torrent file (starts with "d")
        if bytes.first() != Some(&b'd') {
            return Err(TemplateError::InvalidResponse(
                "Invalid torrent file format".to_string()
            ));
        }

        Ok(bytes.to_vec())
    }
}
