//! Transmission RPC client
//!
//! Implements the Transmission RPC protocol
//! Reference: https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md

use super::{
    AddTorrentOptions, BitTorrentClient, ClientConfig, ClientError, Result, TorrentFile,
    TorrentInfo,
};
use async_trait::async_trait;
use base64::Engine;
use reqwest::{Client, StatusCode};
use serde::de::IgnoredAny;
use serde::Deserialize;
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
        let body = json!({ "method": method, "arguments": arguments });
        // A 409 hands out a new session id; resend once with it.
        for attempt in 0..2 {
            let mut request = self.http.post(self.rpc_url()).json(&body);
            if let Some(ref session_id) = *self.session_id.read().await {
                request = request.header("X-Transmission-Session-Id", session_id);
            }
            if let (Some(ref username), Some(ref password)) =
                (&self.config.username, &self.config.password)
            {
                request = request.basic_auth(username, Some(password));
            }

            let response = request.send().await?;
            if response.status() == StatusCode::CONFLICT && attempt == 0 {
                if let Some(session_id) = response.headers().get("X-Transmission-Session-Id") {
                    *self.session_id.write().await =
                        Some(session_id.to_str().unwrap_or("").to_string());
                }
                continue;
            }
            if response.status() == StatusCode::UNAUTHORIZED {
                return Err(ClientError::AuthenticationFailed);
            }
            if !response.status().is_success() {
                return Err(ClientError::InvalidResponse(format!("HTTP {}", response.status())));
            }

            let rpc_response: RpcResponse<T> = response.json().await?;
            if rpc_response.result != "success" {
                return Err(ClientError::InvalidResponse(rpc_response.result));
            }
            return rpc_response
                .arguments
                .ok_or_else(|| ClientError::InvalidResponse("响应里缺少 arguments".to_string()));
        }
        Err(ClientError::InvalidResponse("session id 连续两次被拒".to_string()))
    }
}

#[async_trait]
impl BitTorrentClient for TransmissionClient {
    async fn test_connection(&self) -> Result<bool> {
        let _: SessionStats = self.rpc_call("session-stats", json!({})).await?;
        Ok(true)
    }

    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>> {
        let args = json!({
            "fields": ["hashString", "name", "totalSize", "percentDone", "downloadDir", "trackers"]
        });
        let response: TorrentsResponse = self.rpc_call("torrent-get", args).await?;
        Ok(response.torrents.into_iter().map(Into::into).collect())
    }

    async fn get_piece_hashes(&self, _hash: &str) -> Result<Vec<String>> {
        Err(ClientError::Unsupported("Transmission 不提供 piece 哈希"))
    }

    async fn get_torrent_files(&self, hash: &str) -> Result<Vec<TorrentFile>> {
        let args = json!({ "ids": [hash], "fields": ["files"] });
        let response: FilesResponse = self.rpc_call("torrent-get", args).await?;
        let torrent = response
            .torrents
            .into_iter()
            .next()
            .ok_or_else(|| ClientError::TorrentNotFound(hash.to_string()))?;
        Ok(torrent
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|f| TorrentFile { name: f.name, size: f.length.max(0) as u64 })
            .collect())
    }

    async fn has_torrent(&self, hash: &str) -> Result<bool> {
        let args = json!({ "ids": [hash], "fields": ["hashString"] });
        let response: HashesResponse = self.rpc_call("torrent-get", args).await?;
        Ok(response.torrents.iter().any(|t| t.hash_string.eq_ignore_ascii_case(hash)))
    }

    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<()> {
        // Transmission always verifies existing data before it seeds.
        let args = json!({
            "metainfo": base64::engine::general_purpose::STANDARD.encode(torrent_bytes),
            "paused": true,
            "download-dir": options.save_path,
            "labels": options.tags,
        });
        let response: AddTorrentResponse = self.rpc_call("torrent-add", args).await?;
        match (response.torrent_added, response.torrent_duplicate) {
            // 3.x ignores `labels` on torrent-add, so set them again. Never on a duplicate:
            // torrent-set replaces the labels of what is the user's own torrent.
            (Some(added), _) => {
                let args = json!({ "ids": [added.hash_string], "labels": options.tags });
                let set: Result<IgnoredAny> = self.rpc_call("torrent-set", args).await;
                set.map(|_| ()).map_err(|e| ClientError::LabelsNotSet(e.to_string()))
            }
            (None, Some(_)) => Err(ClientError::Duplicate),
            (None, None) => Err(ClientError::InvalidResponse("torrent-add 没有返回种子".into())),
        }
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
    #[serde(rename = "downloadDir")]
    download_dir: String,
    trackers: Option<Vec<TrTracker>>,
}

#[derive(Debug, Deserialize)]
struct TrTracker {
    announce: String,
}

#[derive(Debug, Deserialize)]
struct FilesResponse {
    torrents: Vec<FilesOnly>,
}

#[derive(Debug, Deserialize)]
struct FilesOnly {
    files: Option<Vec<TrFile>>,
}

#[derive(Debug, Deserialize)]
struct TrFile {
    name: String,
    length: i64,
}

#[derive(Debug, Deserialize)]
struct HashesResponse {
    torrents: Vec<HashOnly>,
}

#[derive(Debug, Deserialize)]
struct HashOnly {
    #[serde(rename = "hashString")]
    hash_string: String,
}

#[derive(Debug, Deserialize)]
struct AddTorrentResponse {
    #[serde(rename = "torrent-added")]
    torrent_added: Option<HashOnly>,
    #[serde(rename = "torrent-duplicate")]
    torrent_duplicate: Option<serde_json::Value>,
}

impl From<TrTorrent> for TorrentInfo {
    fn from(t: TrTorrent) -> Self {
        TorrentInfo {
            hash: t.hash_string.to_lowercase(),
            name: t.name,
            size: t.total_size.max(0) as u64,
            progress: t.percent_done,
            save_path: t.download_dir,
            tracker: t.trackers.and_then(|ts| ts.into_iter().next()).map(|t| t.announce),
        }
    }
}
