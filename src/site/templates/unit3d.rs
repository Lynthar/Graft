//! Unit3D site template
//!
//! Unit3D is a modern PT site framework used by sites like Blutopia, Aither, etc.

use async_trait::async_trait;

use super::{fetch_torrent, Result, SiteTemplate};
use crate::site::Site;

pub struct Unit3DTemplate {
    site: Site,
}

impl Unit3DTemplate {
    pub fn new(site: Site) -> Self {
        Self { site }
    }
}

#[async_trait]
impl SiteTemplate for Unit3DTemplate {
    async fn download_torrent(&self, http_client: &reqwest::Client, torrent_id: &str) -> Result<Vec<u8>> {
        fetch_torrent(http_client, &self.site, torrent_id).await
    }
}
