//! qBittorrent WebUI API client
//!
//! Implements the qBittorrent WebUI API v2.x
//! Reference: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-4.1)

use super::{
    AddTorrentOptions, BitTorrentClient, ClientConfig, ClientError, ClientType, Result,
    TorrentFile, TorrentInfo, TorrentState,
};
use async_trait::async_trait;
use reqwest::{multipart, Client, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct QBittorrentClient {
    config: ClientConfig,
    http: Client,
    cookie: Arc<RwLock<Option<String>>>,
}

impl QBittorrentClient {
    pub fn new(config: ClientConfig) -> Self {
        let http = Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http,
            cookie: Arc::new(RwLock::new(None)),
        }
    }

    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/v2{}", self.config.base_url(), endpoint)
    }

    async fn login(&self) -> Result<()> {
        let url = self.api_url("/auth/login");

        let params = [
            ("username", self.config.username.as_deref().unwrap_or("")),
            ("password", self.config.password.as_deref().unwrap_or("")),
        ];

        let response = self.http.post(&url).form(&params).send().await?;

        if response.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::AuthenticationFailed);
        }

        let text = response.text().await?;
        if text.contains("Fails") || text.contains("fail") {
            return Err(ClientError::AuthenticationFailed);
        }

        // Extract SID cookie
        if let Some(cookie) = self.http.get(&self.api_url("/app/version")).send().await?.headers().get("set-cookie") {
            if let Ok(cookie_str) = cookie.to_str() {
                let mut cookie_guard = self.cookie.write().await;
                *cookie_guard = Some(cookie_str.to_string());
            }
        }

        Ok(())
    }

    async fn ensure_logged_in(&self) -> Result<()> {
        // Try a simple request to check if we're logged in
        let response = self.http.get(&self.api_url("/app/version")).send().await?;

        if response.status() == StatusCode::FORBIDDEN {
            self.login().await?;
        }

        Ok(())
    }
}

#[async_trait]
impl BitTorrentClient for QBittorrentClient {
    fn client_type(&self) -> ClientType {
        ClientType::QBittorrent
    }

    fn client_id(&self) -> &str {
        &self.config.id
    }

    async fn test_connection(&self) -> Result<bool> {
        self.login().await?;

        let response = self.http.get(&self.api_url("/app/version")).send().await?;

        Ok(response.status().is_success())
    }

    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/info");
        let response = self.http.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        let torrents: Vec<QBTorrent> = response.json().await?;

        let mut result = Vec::with_capacity(torrents.len());
        for t in torrents {
            result.push(t.into());
        }

        Ok(result)
    }

    async fn get_torrent(&self, hash: &str) -> Result<Option<TorrentInfo>> {
        self.ensure_logged_in().await?;

        let url = format!("{}?hashes={}", self.api_url("/torrents/info"), hash);
        let response = self.http.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let torrents: Vec<QBTorrent> = response.json().await?;

        Ok(torrents.into_iter().next().map(|t| t.into()))
    }

    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>> {
        self.ensure_logged_in().await?;

        let url = format!("{}?hash={}", self.api_url("/torrents/files"), hash);
        let response = self.http.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::TorrentNotFound(hash.to_string()));
        }

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        let files: Vec<QBTorrentFile> = response.json().await?;

        Ok(files.into_iter().map(|f| f.into()).collect())
    }

    async fn get_torrent_trackers(&self, hash: &str) -> Result<Vec<String>> {
        self.ensure_logged_in().await?;

        let url = format!("{}?hash={}", self.api_url("/torrents/trackers"), hash);
        let response = self.http.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::TorrentNotFound(hash.to_string()));
        }

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        let trackers: Vec<QBTracker> = response.json().await?;

        Ok(trackers
            .into_iter()
            .filter(|t| !t.url.is_empty() && t.url != "** [DHT] **" && t.url != "** [PeX] **")
            .map(|t| t.url)
            .collect())
    }

    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<String> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/add");

        let file_part = multipart::Part::bytes(torrent_bytes.to_vec())
            .file_name("torrent.torrent")
            .mime_str("application/x-bittorrent")
            .map_err(|e| ClientError::InvalidResponse(e.to_string()))?;

        let mut form = multipart::Form::new().part("torrents", file_part);

        if let Some(ref path) = options.save_path {
            form = form.text("savepath", path.clone());
        }

        if let Some(ref category) = options.category {
            form = form.text("category", category.clone());
        }

        if !options.tags.is_empty() {
            form = form.text("tags", options.tags.join(","));
        }

        if options.paused {
            form = form.text("paused", "true");
        }

        if options.skip_checking {
            form = form.text("skip_checking", "true");
        }

        let response = self.http.post(&url).multipart(form).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        // qBittorrent doesn't return the hash directly, we need to parse the torrent
        // For now, return empty string - caller should use torrent parsing to get hash
        Ok(String::new())
    }

    async fn remove_torrent(&self, hash: &str, delete_files: bool) -> Result<()> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/delete");
        let params = [
            ("hashes", hash),
            ("deleteFiles", if delete_files { "true" } else { "false" }),
        ];

        let response = self.http.post(&url).form(&params).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn pause_torrent(&self, hash: &str) -> Result<()> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/pause");
        let params = [("hashes", hash)];

        let response = self.http.post(&url).form(&params).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn resume_torrent(&self, hash: &str) -> Result<()> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/resume");
        let params = [("hashes", hash)];

        let response = self.http.post(&url).form(&params).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn recheck_torrent(&self, hash: &str) -> Result<()> {
        self.ensure_logged_in().await?;

        let url = self.api_url("/torrents/recheck");
        let params = [("hashes", hash)];

        let response = self.http.post(&url).form(&params).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        Ok(())
    }
}

// qBittorrent API response types

#[derive(Debug, Deserialize)]
struct QBTorrent {
    hash: String,
    name: String,
    size: i64,
    progress: f64,
    state: String,
    save_path: String,
    category: Option<String>,
    tags: Option<String>,
    tracker: Option<String>,
    added_on: Option<i64>,
}

impl From<QBTorrent> for TorrentInfo {
    fn from(t: QBTorrent) -> Self {
        let state = match t.state.as_str() {
            "downloading" | "forcedDL" | "metaDL" | "allocating" => TorrentState::Downloading,
            "uploading" | "forcedUP" | "stalledUP" => TorrentState::Seeding,
            "pausedDL" | "pausedUP" => TorrentState::Paused,
            "checkingDL" | "checkingUP" | "checkingResumeData" => TorrentState::Checking,
            "error" | "missingFiles" => TorrentState::Error,
            "queuedDL" | "queuedUP" => TorrentState::Queued,
            "stalledDL" => TorrentState::Stalled,
            _ => TorrentState::Unknown,
        };

        let tags = t
            .tags
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        let added_on = t.added_on.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
        });

        TorrentInfo {
            hash: t.hash.to_lowercase(),
            name: t.name,
            size: t.size as u64,
            progress: t.progress,
            state,
            save_path: t.save_path,
            category: t.category,
            tags,
            tracker: t.tracker,
            trackers: Vec::new(), // Will be fetched separately if needed
            added_on,
            files: Vec::new(), // Will be fetched separately if needed
        }
    }
}

#[derive(Debug, Deserialize)]
struct QBTorrentFile {
    name: String,
    size: i64,
    progress: f64,
}

impl From<QBTorrentFile> for TorrentFile {
    fn from(f: QBTorrentFile) -> Self {
        TorrentFile {
            name: f.name,
            size: f.size as u64,
            progress: f.progress,
        }
    }
}

#[derive(Debug, Deserialize)]
struct QBTracker {
    url: String,
}
