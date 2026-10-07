//! qBittorrent WebUI API client (API v2, qBittorrent 4.1 and later)

use super::{
    AddTorrentOptions, BitTorrentClient, ClientConfig, ClientError, Result, TorrentFile,
    TorrentInfo,
};
use async_trait::async_trait;
use reqwest::{multipart, Client, RequestBuilder, Response, StatusCode};
use serde::Deserialize;

pub struct QBittorrentClient {
    config: ClientConfig,
    http: Client,
}

impl QBittorrentClient {
    pub fn new(config: ClientConfig) -> Self {
        let http = Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, http }
    }

    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/v2{}", self.config.base_url(), endpoint)
    }

    async fn login(&self) -> Result<()> {
        let params = [
            ("username", self.config.username.as_deref().unwrap_or("")),
            ("password", self.config.password.as_deref().unwrap_or("")),
        ];
        let response = self.http.post(self.api_url("/auth/login")).form(&params).send().await?;
        if response.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::AuthenticationFailed);
        }
        let text = response.text().await?;
        if text.contains("Fails") {
            return Err(ClientError::AuthenticationFailed);
        }
        Ok(())
    }

    /// Send a request, logging in and retrying once if the session is missing or expired.
    async fn send(&self, build: impl Fn(&Client) -> Result<RequestBuilder>) -> Result<Response> {
        let response = build(&self.http)?.send().await?;
        if response.status() != StatusCode::FORBIDDEN {
            return Ok(response);
        }
        self.login().await?;
        Ok(build(&self.http)?.send().await?)
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let url = self.api_url(endpoint);
        let response = self.send(|http| Ok(http.get(&url).query(query))).await?;
        if response.status() == StatusCode::NOT_FOUND {
            let hash = query.iter().find(|(k, _)| *k == "hash").map(|(_, v)| *v);
            return Err(ClientError::TorrentNotFound(hash.unwrap_or_default().to_string()));
        }
        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!("HTTP {}", response.status())));
        }
        Ok(response.json().await?)
    }
}

#[async_trait]
impl BitTorrentClient for QBittorrentClient {
    async fn test_connection(&self) -> Result<bool> {
        self.login().await?;
        let response = self.http.get(self.api_url("/app/version")).send().await?;
        Ok(response.status().is_success())
    }

    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>> {
        let torrents: Vec<QBTorrent> = self.get_json("/torrents/info", &[]).await?;
        Ok(torrents.into_iter().map(Into::into).collect())
    }

    async fn get_piece_hashes(&self, hash: &str) -> Result<Vec<String>> {
        self.get_json("/torrents/pieceHashes", &[("hash", hash)]).await
    }

    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>> {
        let files: Vec<QBTorrentFile> = self.get_json("/torrents/files", &[("hash", hash)]).await?;
        Ok(files
            .into_iter()
            .map(|f| TorrentFile { name: f.name, size: f.size.max(0) as u64 })
            .collect())
    }

    async fn has_torrent(&self, hash: &str) -> Result<bool> {
        let torrents: Vec<QBTorrent> = self.get_json("/torrents/info", &[("hashes", hash)]).await?;
        Ok(torrents.iter().any(|t| t.hash.eq_ignore_ascii_case(hash)))
    }

    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<()> {
        let url = self.api_url("/torrents/add");
        let tags = options.tags.join(",");
        let response = self
            .send(|http| {
                let file = multipart::Part::bytes(torrent_bytes.to_vec())
                    .file_name("reseed.torrent")
                    .mime_str("application/x-bittorrent")
                    .map_err(|e| ClientError::InvalidResponse(e.to_string()))?;
                // 4.x reads `paused`, 5.x reads `stopped`; each ignores the other.
                let form = multipart::Form::new()
                    .part("torrents", file)
                    .text("savepath", options.save_path.clone())
                    .text("autoTMM", "false")
                    .text("contentLayout", "Original")
                    .text("paused", "true")
                    .text("stopped", "true")
                    .text("tags", tags.clone());
                Ok(http.post(&url).multipart(form))
            })
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        match status {
            StatusCode::CONFLICT => Err(ClientError::Duplicate),
            s if s.is_success() && body.trim() != "Fails." => Ok(()),
            s => Err(ClientError::InvalidResponse(format!("HTTP {s}，{}", body.trim()))),
        }
    }
}

#[derive(Debug, Deserialize)]
struct QBTorrent {
    hash: String,
    name: String,
    total_size: i64,
    progress: f64,
    save_path: String,
    tracker: Option<String>,
}

impl From<QBTorrent> for TorrentInfo {
    fn from(t: QBTorrent) -> Self {
        TorrentInfo {
            hash: t.hash.to_lowercase(),
            name: t.name,
            size: t.total_size.max(0) as u64,
            progress: t.progress,
            save_path: t.save_path,
            tracker: t.tracker.filter(|t| !t.is_empty()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct QBTorrentFile {
    name: String,
    size: i64,
}
