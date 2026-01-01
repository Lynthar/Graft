//! Unit3D site template
//!
//! Unit3D is a modern PT site framework used by sites like Blutopia, Aither, etc.

use async_trait::async_trait;

use super::{Result, SiteTemplate, TemplateError, TemplateType};
use crate::site::SiteConfig;

pub struct Unit3DTemplate {
    config: SiteConfig,
}

impl Unit3DTemplate {
    pub fn new(config: SiteConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SiteTemplate for Unit3DTemplate {
    fn config(&self) -> &SiteConfig {
        &self.config
    }

    fn template_type(&self) -> TemplateType {
        TemplateType::Unit3D
    }

    fn build_download_url(&self, torrent_id: &str) -> Result<String> {
        let passkey = self.config.passkey.as_ref()
            .ok_or(TemplateError::MissingPasskey)?;

        // Unit3D typically uses format: /torrent/download/{id}.{rsskey}
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
            return Err(TemplateError::InvalidResponse(
                "Invalid torrent file format".to_string()
            ));
        }

        Ok(bytes.to_vec())
    }
}
