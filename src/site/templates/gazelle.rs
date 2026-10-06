//! Gazelle site template
//!
//! Gazelle is a PT framework commonly used by music trackers like Redacted, Orpheus.

use async_trait::async_trait;

use super::{fetch_torrent, Result, SiteTemplate};
use crate::site::Site;

pub struct GazelleTemplate {
    site: Site,
}

impl GazelleTemplate {
    pub fn new(site: Site) -> Self {
        Self { site }
    }
}

#[async_trait]
impl SiteTemplate for GazelleTemplate {
    async fn download_torrent(&self, http_client: &reqwest::Client, torrent_id: &str) -> Result<Vec<u8>> {
        fetch_torrent(http_client, &self.site, torrent_id).await
    }
}
