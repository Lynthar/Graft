//! NexusPHP site template
//!
//! NexusPHP is the most common PT site framework, used by many Chinese PT sites.

use async_trait::async_trait;

use super::{fetch_torrent, Result, SiteTemplate};
use crate::site::Site;

pub struct NexusPHPTemplate {
    site: Site,
}

impl NexusPHPTemplate {
    pub fn new(site: Site) -> Self {
        Self { site }
    }
}

#[async_trait]
impl SiteTemplate for NexusPHPTemplate {
    async fn download_torrent(&self, http_client: &reqwest::Client, torrent_id: &str) -> Result<Vec<u8>> {
        fetch_torrent(http_client, &self.site, torrent_id).await
    }
}
