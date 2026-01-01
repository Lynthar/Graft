//! Transmission RPC client
//!
//! Implements the Transmission RPC protocol
//! Reference: https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md

use super::{
    AddTorrentOptions, BitTorrentClient, ClientConfig, ClientError, ClientType, Result,
    TorrentFile, TorrentInfo, TorrentState,
};
use async_trait::async_trait;
use base64::Engine;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TransmissionClient {
    config: ClientConfig,
    http: Client,
    session_id: Arc<RwLock<Option<String>>>,
}

impl TransmissionClient {
    pub fn new(config: ClientConfig) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http,
            session_id: Arc::new(RwLock::new(None)),
        }
    }

    fn rpc_url(&self) -> String {
        format!("{}/transmission/rpc", self.config.base_url())
    }

    async fn rpc_call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        arguments: serde_json::Value,
    ) -> Result<T> {
        let url = self.rpc_url();
        let body = json!({
            "method": method,
            "arguments": arguments,
        });

        let mut request = self.http.post(&url).json(&body);

        // Add session ID if available
        if let Some(ref session_id) = *self.session_id.read().await {
            request = request.header("X-Transmission-Session-Id", session_id);
        }

        // Add basic auth if credentials provided
        if let (Some(ref username), Some(ref password)) =
            (&self.config.username, &self.config.password)
        {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;

        // Handle CSRF token
        if response.status() == StatusCode::CONFLICT {
            if let Some(session_id) = response.headers().get("X-Transmission-Session-Id") {
                let mut guard = self.session_id.write().await;
                *guard = Some(session_id.to_str().unwrap_or("").to_string());
            }
            // Retry with new session ID
            return Box::pin(self.rpc_call(method, arguments)).await;
        }

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(ClientError::AuthenticationFailed);
        }

        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse(format!(
                "Status: {}",
                response.status()
            )));
        }

        let rpc_response: RpcResponse<T> = response.json().await?;

        if rpc_response.result != "success" {
            return Err(ClientError::InvalidResponse(rpc_response.result));
        }

        rpc_response
            .arguments
            .ok_or_else(|| ClientError::InvalidResponse("Missing arguments".to_string()))
    }
}

#[async_trait]
impl BitTorrentClient for TransmissionClient {
    fn client_type(&self) -> ClientType {
        ClientType::Transmission
    }

    fn client_id(&self) -> &str {
        &self.config.id
    }

    async fn test_connection(&self) -> Result<bool> {
        let _: SessionStats = self.rpc_call("session-stats", json!({})).await?;
        Ok(true)
    }

    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>> {
        let args = json!({
            "fields": [
                "id", "hashString", "name", "totalSize", "percentDone",
                "status", "downloadDir", "labels", "trackers", "addedDate", "files"
            ]
        });

        let response: TorrentsResponse = self.rpc_call("torrent-get", args).await?;

        Ok(response.torrents.into_iter().map(|t| t.into()).collect())
    }

    async fn get_torrent(&self, hash: &str) -> Result<Option<TorrentInfo>> {
        let args = json!({
            "ids": [hash],
            "fields": [
                "id", "hashString", "name", "totalSize", "percentDone",
                "status", "downloadDir", "labels", "trackers", "addedDate", "files"
            ]
        });

        let response: TorrentsResponse = self.rpc_call("torrent-get", args).await?;

        Ok(response.torrents.into_iter().next().map(|t| t.into()))
    }

    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>> {
        let args = json!({
            "ids": [hash],
            "fields": ["files", "fileStats"]
        });

        let response: TorrentsResponse = self.rpc_call("torrent-get", args).await?;

        let torrent = response
            .torrents
            .into_iter()
            .next()
            .ok_or_else(|| ClientError::TorrentNotFound(hash.to_string()))?;

        Ok(torrent
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|f| TorrentFile {
                name: f.name,
                size: f.length as u64,
                progress: f.bytes_completed as f64 / f.length as f64,
            })
            .collect())
    }

    async fn get_torrent_trackers(&self, hash: &str) -> Result<Vec<String>> {
        let args = json!({
            "ids": [hash],
            "fields": ["trackers"]
        });

        let response: TorrentsResponse = self.rpc_call("torrent-get", args).await?;

        let torrent = response
            .torrents
            .into_iter()
            .next()
            .ok_or_else(|| ClientError::TorrentNotFound(hash.to_string()))?;

        Ok(torrent
            .trackers
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.announce)
            .collect())
    }

    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<String> {
        let metainfo = base64::engine::general_purpose::STANDARD.encode(torrent_bytes);

        let mut args = json!({
            "metainfo": metainfo,
            "paused": options.paused,
        });

        if let Some(ref path) = options.save_path {
            args["download-dir"] = json!(path);
        }

        if !options.tags.is_empty() {
            args["labels"] = json!(options.tags);
        }

        let response: AddTorrentResponse = self.rpc_call("torrent-add", args).await?;

        Ok(response
            .torrent_added
            .or(response.torrent_duplicate)
            .map(|t| t.hash_string)
            .unwrap_or_default())
    }

    async fn remove_torrent(&self, hash: &str, delete_files: bool) -> Result<()> {
        let args = json!({
            "ids": [hash],
            "delete-local-data": delete_files,
        });

        let _: serde_json::Value = self.rpc_call("torrent-remove", args).await?;
        Ok(())
    }

    async fn pause_torrent(&self, hash: &str) -> Result<()> {
        let args = json!({ "ids": [hash] });
        let _: serde_json::Value = self.rpc_call("torrent-stop", args).await?;
        Ok(())
    }

    async fn resume_torrent(&self, hash: &str) -> Result<()> {
        let args = json!({ "ids": [hash] });
        let _: serde_json::Value = self.rpc_call("torrent-start", args).await?;
        Ok(())
    }

    async fn recheck_torrent(&self, hash: &str) -> Result<()> {
        let args = json!({ "ids": [hash] });
        let _: serde_json::Value = self.rpc_call("torrent-verify", args).await?;
        Ok(())
    }
}

// Transmission RPC response types

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: String,
    arguments: Option<T>,
}

#[derive(Debug, Deserialize)]
struct SessionStats {
    #[allow(dead_code)]
    #[serde(rename = "activeTorrentCount")]
    active_torrent_count: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TorrentsResponse {
    torrents: Vec<TrTorrent>,
}

#[derive(Debug, Deserialize)]
struct TrTorrent {
    #[serde(rename = "hashString")]
    hash_string: String,
    name: String,
    #[serde(rename = "totalSize")]
    total_size: i64,
    #[serde(rename = "percentDone")]
    percent_done: f64,
    status: i32,
    #[serde(rename = "downloadDir")]
    download_dir: String,
    labels: Option<Vec<String>>,
    trackers: Option<Vec<TrTracker>>,
    #[serde(rename = "addedDate")]
    added_date: Option<i64>,
    files: Option<Vec<TrFile>>,
}

#[derive(Debug, Deserialize)]
struct TrTracker {
    announce: String,
}

#[derive(Debug, Deserialize)]
struct TrFile {
    name: String,
    length: i64,
    #[serde(rename = "bytesCompleted")]
    bytes_completed: i64,
}

#[derive(Debug, Deserialize)]
struct AddTorrentResponse {
    #[serde(rename = "torrent-added")]
    torrent_added: Option<AddedTorrent>,
    #[serde(rename = "torrent-duplicate")]
    torrent_duplicate: Option<AddedTorrent>,
}

#[derive(Debug, Deserialize)]
struct AddedTorrent {
    #[serde(rename = "hashString")]
    hash_string: String,
}

impl From<TrTorrent> for TorrentInfo {
    fn from(t: TrTorrent) -> Self {
        // Transmission status codes:
        // 0 = stopped, 1 = queued to verify, 2 = verifying, 3 = queued to download
        // 4 = downloading, 5 = queued to seed, 6 = seeding
        let state = match t.status {
            0 => TorrentState::Paused,
            1 | 2 => TorrentState::Checking,
            3 | 4 => TorrentState::Downloading,
            5 | 6 => TorrentState::Seeding,
            _ => TorrentState::Unknown,
        };

        let added_on = t.added_date.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
        });

        let trackers: Vec<String> = t
            .trackers
            .as_ref()
            .map(|ts| ts.iter().map(|t| t.announce.clone()).collect())
            .unwrap_or_default();

        let files: Vec<TorrentFile> = t
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|f| TorrentFile {
                name: f.name,
                size: f.length as u64,
                progress: if f.length > 0 {
                    f.bytes_completed as f64 / f.length as f64
                } else {
                    0.0
                },
            })
            .collect();

        TorrentInfo {
            hash: t.hash_string.to_lowercase(),
            name: t.name,
            size: t.total_size as u64,
            progress: t.percent_done,
            state,
            save_path: t.download_dir,
            category: None, // Transmission doesn't have categories
            tags: t.labels.unwrap_or_default(),
            tracker: trackers.first().cloned(),
            trackers,
            added_on,
            files,
        }
    }
}
