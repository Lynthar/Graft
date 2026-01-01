//! Index service for importing torrents from download clients
//!
//! This service scans download clients, identifies sites from tracker URLs,
//! and builds a local index with content fingerprints for cross-site matching.

use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::client::{BitTorrentClient, TorrentInfo};
use crate::db::Database;
use crate::service::fingerprint::{ContentFingerprint, FingerprintEntry, FingerprintMatcher};
use crate::site::TrackerIdentifier;

/// Index service for managing the torrent index
pub struct IndexService {
    db: Database,
    tracker_identifier: Arc<TrackerIdentifier>,
}

impl IndexService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            tracker_identifier: Arc::new(TrackerIdentifier::new()),
        }
    }

    /// Import torrents from a download client into the index
    pub async fn import_from_client(
        &self,
        client: &dyn BitTorrentClient,
        client_id: &str,
    ) -> Result<ImportResult> {
        info!("Starting import from client: {}", client_id);

        let torrents = client.get_torrents().await
            .context("Failed to get torrents from client")?;

        info!("Found {} torrents in client", torrents.len());

        let mut result = ImportResult::default();

        for torrent in &torrents {
            result.total += 1;

            // Get tracker URLs for site identification
            let trackers = if torrent.trackers.is_empty() {
                match client.get_torrent_trackers(&torrent.hash).await {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("Failed to get trackers for {}: {}", torrent.hash, e);
                        Vec::new()
                    }
                }
            } else {
                torrent.trackers.clone()
            };

            // Identify site from trackers
            let site_info = match self.tracker_identifier.identify_from_trackers(&trackers) {
                Some(info) => info,
                None => {
                    result.unrecognized += 1;
                    continue;
                }
            };

            // Get files for fingerprint calculation
            let files = if torrent.files.is_empty() {
                match client.get_torrent_files(&torrent.hash).await {
                    Ok(f) => f,
                    Err(e) => {
                        warn!("Failed to get files for {}: {}", torrent.hash, e);
                        Vec::new()
                    }
                }
            } else {
                torrent.files.clone()
            };

            // Calculate fingerprint
            let fingerprint = if files.is_empty() {
                // Fallback: use torrent size info
                ContentFingerprint::from_size(torrent.size, 1, torrent.size)
            } else {
                ContentFingerprint::from_files(&files)
            };

            // Check if already exists
            if self.exists(&torrent.hash, &site_info.site_id)? {
                result.skipped += 1;
                continue;
            }

            // Insert into database
            self.insert_entry(
                &torrent.hash,
                &site_info.site_id,
                site_info.torrent_id.as_deref(),
                &fingerprint,
                Some(&torrent.name),
                Some(&torrent.save_path),
                Some(client_id),
            )?;

            result.imported += 1;
        }

        info!(
            "Import complete: {} total, {} imported, {} skipped, {} unrecognized",
            result.total, result.imported, result.skipped, result.unrecognized
        );

        Ok(result)
    }

    /// Check if an entry already exists
    fn exists(&self, info_hash: &str, site_id: &str) -> Result<bool> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT 1 FROM torrent_index WHERE info_hash = ?1 AND site_id = ?2 LIMIT 1"
        )?;

        Ok(stmt.exists([info_hash, site_id])?)
    }

    /// Insert a new index entry
    fn insert_entry(
        &self,
        info_hash: &str,
        site_id: &str,
        torrent_id: Option<&str>,
        fingerprint: &ContentFingerprint,
        name: Option<&str>,
        save_path: Option<&str>,
        source_client: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn();

        // First, insert or get fingerprint ID
        let fingerprint_id = self.get_or_create_fingerprint(&conn, fingerprint)?;

        // Insert torrent index entry
        conn.execute(
            "INSERT INTO torrent_index (info_hash, site_id, torrent_id, fingerprint_id, name, size, save_path, source_client)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                info_hash,
                site_id,
                torrent_id,
                fingerprint_id,
                name,
                fingerprint.total_size as i64,
                save_path,
                source_client,
            ],
        )?;

        Ok(())
    }

    fn get_or_create_fingerprint(
        &self,
        conn: &rusqlite::Connection,
        fingerprint: &ContentFingerprint,
    ) -> Result<i64> {
        // Try to find existing fingerprint
        let mut stmt = conn.prepare(
            "SELECT id FROM content_fingerprints
             WHERE total_size = ?1 AND file_count = ?2 AND largest_file_size = ?3
             LIMIT 1"
        )?;

        let result: Option<i64> = stmt
            .query_row(
                rusqlite::params![
                    fingerprint.total_size as i64,
                    fingerprint.file_count as i64,
                    fingerprint.largest_file_size as i64,
                ],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = result {
            return Ok(id);
        }

        // Create new fingerprint
        conn.execute(
            "INSERT INTO content_fingerprints (total_size, file_count, largest_file_size, files_hash)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                fingerprint.total_size as i64,
                fingerprint.file_count as i64,
                fingerprint.largest_file_size as i64,
                fingerprint.files_hash,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Build a fingerprint matcher from the index
    pub fn build_matcher(&self) -> Result<FingerprintMatcher> {
        let conn = self.db.conn();
        let mut matcher = FingerprintMatcher::new();

        let mut stmt = conn.prepare(
            "SELECT ti.info_hash, ti.site_id, ti.torrent_id, ti.name, ti.save_path,
                    cf.total_size, cf.file_count, cf.largest_file_size, cf.files_hash
             FROM torrent_index ti
             JOIN content_fingerprints cf ON ti.fingerprint_id = cf.id"
        )?;

        let entries = stmt.query_map([], |row| {
            let fingerprint = ContentFingerprint {
                total_size: row.get::<_, i64>(5)? as u64,
                file_count: row.get::<_, i64>(6)? as usize,
                largest_file_size: row.get::<_, i64>(7)? as u64,
                files_hash: row.get(8)?,
            };

            Ok(FingerprintEntry {
                fingerprint,
                info_hash: row.get(0)?,
                site_id: row.get(1)?,
                torrent_id: row.get(2)?,
                name: row.get(3)?,
                save_path: row.get(4)?,
            })
        })?;

        for entry in entries {
            matcher.add(entry?);
        }

        Ok(matcher)
    }

    /// Get index statistics
    pub fn get_stats(&self) -> Result<IndexStats> {
        let conn = self.db.conn();

        let total_entries: i64 = conn.query_row(
            "SELECT COUNT(*) FROM torrent_index",
            [],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT site_id, COUNT(*) as count FROM torrent_index GROUP BY site_id ORDER BY count DESC"
        )?;

        let sites = stmt
            .query_map([], |row| {
                Ok(SiteIndexCount {
                    site_id: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(IndexStats {
            total_entries,
            sites,
        })
    }

    /// Clear all index entries
    pub fn clear(&self) -> Result<()> {
        let conn = self.db.conn();
        conn.execute("DELETE FROM torrent_index", [])?;
        conn.execute("DELETE FROM content_fingerprints", [])?;
        Ok(())
    }

    /// Clear index entries for a specific site
    pub fn clear_by_site(&self, site_id: &str) -> Result<()> {
        let conn = self.db.conn();
        conn.execute("DELETE FROM torrent_index WHERE site_id = ?1", [site_id])?;
        Ok(())
    }
}

/// Result of an import operation
#[derive(Debug, Default, Serialize)]
pub struct ImportResult {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub unrecognized: usize,
}

/// Index statistics
#[derive(Debug, Serialize)]
pub struct IndexStats {
    pub total_entries: i64,
    pub sites: Vec<SiteIndexCount>,
}

/// Count of index entries per site
#[derive(Debug, Serialize)]
pub struct SiteIndexCount {
    pub site_id: String,
    pub count: i64,
}
