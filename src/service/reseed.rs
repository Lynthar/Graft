//! Reseed service for cross-site seeding
//!
//! This service matches torrents across sites using content fingerprints
//! and handles the actual reseed operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::client::{AddTorrentOptions, BitTorrentClient, ClientConfig};
use crate::db::Database;
use crate::service::fingerprint::{ContentFingerprint, FingerprintMatcher, MatchResult};
use crate::service::index::IndexService;
use crate::site::{SiteConfig, SiteTemplate};

/// Reseed service
pub struct ReseedService {
    db: Database,
    index_service: Arc<IndexService>,
    http_client: reqwest::Client,
    request_interval: Duration,
}

impl ReseedService {
    pub fn new(db: Database, index_service: Arc<IndexService>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            db,
            index_service,
            http_client,
            request_interval: Duration::from_millis(500),
        }
    }

    pub fn with_request_interval(mut self, interval: Duration) -> Self {
        self.request_interval = interval;
        self
    }

    /// Preview reseed matches without executing
    pub async fn preview(
        &self,
        source_client: &dyn BitTorrentClient,
        target_sites: &[SiteConfig],
    ) -> Result<PreviewResult> {
        info!("Starting reseed preview");

        // Get torrents from source client
        let torrents = source_client.get_torrents().await
            .context("Failed to get torrents from source client")?;

        info!("Source client has {} torrents", torrents.len());

        // Build matcher from index
        let matcher = self.index_service.build_matcher()?;

        info!("Index has {} entries", matcher.len());

        // Find matches
        let target_site_ids: HashSet<_> = target_sites.iter().map(|s| s.id.clone()).collect();
        let mut matches = Vec::new();

        for torrent in &torrents {
            // Get files for fingerprint
            let files = if torrent.files.is_empty() {
                source_client.get_torrent_files(&torrent.hash).await.unwrap_or_default()
            } else {
                torrent.files.clone()
            };

            let fingerprint = if files.is_empty() {
                ContentFingerprint::from_size(torrent.size, 1, torrent.size)
            } else {
                ContentFingerprint::from_files(&files)
            };

            // Find cross-site matches
            // We need to identify the source site first
            let trackers = if torrent.trackers.is_empty() {
                source_client.get_torrent_trackers(&torrent.hash).await.unwrap_or_default()
            } else {
                torrent.trackers.clone()
            };

            let source_site = crate::site::TrackerIdentifier::new()
                .identify_from_trackers(&trackers)
                .map(|i| i.site_id);

            // Find matches in target sites
            for matched in matcher.find_matches(&fingerprint) {
                // Skip if same site as source
                if let Some(ref source) = source_site {
                    if &matched.entry.site_id == source {
                        continue;
                    }
                }

                // Skip if not in target sites
                if !target_site_ids.contains(&matched.entry.site_id) {
                    continue;
                }

                matches.push(ReseedMatch {
                    source_hash: torrent.hash.clone(),
                    source_name: torrent.name.clone(),
                    source_site: source_site.clone(),
                    target_site: matched.entry.site_id.clone(),
                    target_torrent_id: matched.entry.torrent_id.clone(),
                    target_hash: matched.entry.info_hash.clone(),
                    save_path: torrent.save_path.clone(),
                    size: torrent.size,
                    confidence: matched.match_result.confidence(),
                });
            }
        }

        let total_size: u64 = matches.iter().map(|m| m.size).sum();

        Ok(PreviewResult {
            matches,
            total_size,
        })
    }

    /// Execute reseed operation
    pub async fn execute(
        &self,
        request: ReseedRequest,
        source_client: &dyn BitTorrentClient,
        target_client: &dyn BitTorrentClient,
        sites: &[SiteConfig],
    ) -> Result<ReseedResult> {
        info!("Starting reseed execution");

        // Get preview first
        let preview = self.preview(source_client, sites).await?;

        info!("Found {} potential matches", preview.matches.len());

        // Get existing hashes in target client to avoid duplicates
        let existing_hashes: HashSet<String> = target_client
            .get_torrents()
            .await?
            .into_iter()
            .map(|t| t.hash.to_lowercase())
            .collect();

        let mut result = ReseedResult::default();
        let sites_map: std::collections::HashMap<_, _> = sites.iter()
            .map(|s| (s.id.clone(), s))
            .collect();

        for m in preview.matches {
            result.total += 1;

            // Check if already exists in target
            if existing_hashes.contains(&m.target_hash.to_lowercase()) {
                result.skipped += 1;
                continue;
            }

            // Get site config
            let site = match sites_map.get(&m.target_site) {
                Some(s) => *s,
                None => {
                    warn!("Site config not found for: {}", m.target_site);
                    result.failed += 1;
                    self.record_history(
                        request.task_id.as_deref(),
                        &m,
                        "failed",
                        Some("Site config not found"),
                    )?;
                    continue;
                }
            };

            // Check passkey
            if site.passkey.is_none() {
                warn!("No passkey configured for site: {}", m.target_site);
                result.failed += 1;
                self.record_history(
                    request.task_id.as_deref(),
                    &m,
                    "failed",
                    Some("No passkey configured"),
                )?;
                continue;
            }

            // Get torrent ID
            let torrent_id = match &m.target_torrent_id {
                Some(id) => id.clone(),
                None => {
                    warn!("No torrent ID available for: {}", m.source_name);
                    result.failed += 1;
                    self.record_history(
                        request.task_id.as_deref(),
                        &m,
                        "failed",
                        Some("No torrent ID available"),
                    )?;
                    continue;
                }
            };

            // Download torrent file
            let template = site.create_template();
            let torrent_bytes = match template.download_torrent(&self.http_client, &torrent_id).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to download torrent {}: {}", torrent_id, e);
                    result.failed += 1;
                    self.record_history(
                        request.task_id.as_deref(),
                        &m,
                        "failed",
                        Some(&format!("Download failed: {}", e)),
                    )?;
                    continue;
                }
            };

            // Add to target client
            let options = AddTorrentOptions {
                save_path: Some(m.save_path.clone()),
                paused: request.add_paused,
                skip_checking: request.skip_checking,
                ..Default::default()
            };

            match target_client.add_torrent(&torrent_bytes, options).await {
                Ok(_) => {
                    info!("Successfully reseeded: {} -> {}", m.source_name, m.target_site);
                    result.success += 1;
                    self.record_history(
                        request.task_id.as_deref(),
                        &m,
                        "success",
                        None,
                    )?;
                }
                Err(e) => {
                    warn!("Failed to add torrent: {}", e);
                    result.failed += 1;
                    self.record_history(
                        request.task_id.as_deref(),
                        &m,
                        "failed",
                        Some(&format!("Add failed: {}", e)),
                    )?;
                }
            }

            // Rate limiting
            tokio::time::sleep(self.request_interval).await;
        }

        info!(
            "Reseed complete: {} total, {} success, {} failed, {} skipped",
            result.total, result.success, result.failed, result.skipped
        );

        Ok(result)
    }

    fn record_history(
        &self,
        task_id: Option<&str>,
        m: &ReseedMatch,
        status: &str,
        message: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO reseed_history (task_id, info_hash, source_site, target_site, status, message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                task_id,
                m.source_hash,
                m.source_site,
                m.target_site,
                status,
                message,
            ],
        )?;
        Ok(())
    }
}

/// Reseed request
#[derive(Debug, Clone, Deserialize)]
pub struct ReseedRequest {
    pub task_id: Option<String>,
    pub source_client_id: String,
    pub target_client_id: String,
    pub target_site_ids: Vec<String>,
    #[serde(default)]
    pub add_paused: bool,
    #[serde(default)]
    pub skip_checking: bool,
}

/// Preview result
#[derive(Debug, Serialize)]
pub struct PreviewResult {
    pub matches: Vec<ReseedMatch>,
    pub total_size: u64,
}

/// A reseed match
#[derive(Debug, Clone, Serialize)]
pub struct ReseedMatch {
    pub source_hash: String,
    pub source_name: String,
    pub source_site: Option<String>,
    pub target_site: String,
    pub target_torrent_id: Option<String>,
    pub target_hash: String,
    pub save_path: String,
    pub size: u64,
    pub confidence: f64,
}

/// Reseed execution result
#[derive(Debug, Default, Serialize)]
pub struct ReseedResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}
