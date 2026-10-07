//! Reseeding: find the source client's content on target sites by pieces hash, then add
//! the matching torrents to a target client, stopped, after verifying what was downloaded.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context};
use serde::Serialize;
use tracing::{info, warn};

use crate::client::{AddTorrentOptions, BitTorrentClient, ClientConfig, ClientError, TorrentInfo};
use crate::db::Database;
use crate::service::tasks::Task;
use crate::site::gate::RateGates;
use crate::site::lookup::{self, LookupError};
use crate::site::{self, Site, TemplateType};
use crate::torrent::{self, sha1_hex};

/// Tag every reseeded torrent carries in the downloader, so they can be filtered as a group.
pub const TAG: &str = "graft";

/// Previews kept for execution; older ones must be run again.
const KEEP_PREVIEWS: usize = 10;

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub source_client_id: String,
    pub read: ReadReport,
    pub sites: Vec<SiteReport>,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReadReport {
    pub total: usize,
    pub incomplete: usize,
    pub recognized: BTreeMap<String, usize>,
    pub unrecognized: Vec<Reason>,
    pub without_pieces: Vec<Reason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reason {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteReport {
    pub site_id: String,
    pub queried: usize,
    pub found: usize,
    pub already_seeding: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub id: usize,
    pub source_hash: String,
    pub source_name: String,
    pub source_site: Option<String>,
    pub save_path: String,
    pub size: u64,
    pub pieces_hash: String,
    pub target_site: String,
    pub target_torrent_id: String,
    /// The site indexes the same `info.pieces` as the local torrent.
    pub evidence: &'static str,
    /// Must be confirmed one by one before execution may include it.
    pub needs_confirmation: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemResult {
    pub candidate_id: usize,
    pub target_site: String,
    pub source_name: String,
    pub status: &'static str,
    pub step: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteSummary {
    pub run_id: String,
    pub success: usize,
    pub skipped: usize,
    pub failed: usize,
    pub not_attempted: usize,
    pub items: Vec<ItemResult>,
}

/// Everything an execution needs, validated by the caller.
pub struct ExecuteRun {
    pub candidates: Vec<Candidate>,
    pub source: ClientConfig,
    pub target: ClientConfig,
    pub busy: BusyGuard,
}

/// Holds the "one execution per target client" lock until dropped.
pub struct BusyGuard {
    busy: Arc<Mutex<HashSet<String>>>,
    client_id: String,
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.busy.lock().expect("busy set poisoned").remove(&self.client_id);
    }
}

pub struct ReseedService {
    db: Database,
    /// For lookups: a redirect there means "go and log in" and must stay visible.
    http: reqwest::Client,
    /// For downloads: sites may redirect `download.php` before serving the file.
    downloads: reqwest::Client,
    gates: RateGates,
    previews: Mutex<VecDeque<(String, Arc<Preview>)>>,
    busy: Arc<Mutex<HashSet<String>>>,
}

struct Outcome {
    status: &'static str,
    step: &'static str,
    message: String,
    target_hash: Option<String>,
    downloaded: bool,
}

impl Outcome {
    fn new(status: &'static str, step: &'static str, message: impl Into<String>) -> Self {
        Outcome { status, step, message: message.into(), target_hash: None, downloaded: false }
    }
}

impl ReseedService {
    pub fn new(db: Database) -> Self {
        let build = |redirects| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(redirects)
                .build()
                .expect("Failed to create HTTP client")
        };
        Self {
            db,
            http: build(reqwest::redirect::Policy::none()),
            downloads: build(reqwest::redirect::Policy::limited(5)),
            gates: RateGates::default(),
            previews: Mutex::new(VecDeque::new()),
            busy: Arc::default(),
        }
    }

    pub fn preview_result(&self, task_id: &str) -> Option<Arc<Preview>> {
        let previews = self.previews.lock().expect("preview list poisoned");
        previews.iter().find(|(id, _)| id == task_id).map(|(_, p)| p.clone())
    }

    /// Claim `client_id` for one execution; `None` while another one is running there.
    pub fn claim_target(&self, client_id: &str) -> Option<BusyGuard> {
        let mut busy = self.busy.lock().expect("busy set poisoned");
        busy.insert(client_id.to_string()).then(|| BusyGuard {
            busy: self.busy.clone(),
            client_id: client_id.to_string(),
        })
    }

    /// Read the source client, then ask every target site which contents it carries.
    pub async fn preview(&self, task: &Task, source: ClientConfig, targets: Vec<Site>) -> anyhow::Result<Preview> {
        let client = source.create_client();
        task.progress("reading the source client", 0, 0);
        let torrents = client.get_torrents().await.context("could not read the source client")?;
        let all_sites = site::load_all(&self.db.conn())?;

        let mut report = ReadReport { total: torrents.len(), ..Default::default() };
        let mut unrecognized: BTreeMap<String, usize> = BTreeMap::new();
        let mut complete: Vec<(TorrentInfo, Option<String>)> = Vec::new();
        for t in torrents {
            if t.progress < 1.0 {
                report.incomplete += 1;
                continue;
            }
            let site = t.tracker.as_deref().and_then(|url| site::recognize(&all_sites, url));
            match (site, &t.tracker) {
                (Some(s), _) => *report.recognized.entry(s.id.clone()).or_default() += 1,
                (None, None) => *unrecognized.entry("no working tracker".into()).or_default() += 1,
                (None, Some(url)) => {
                    let host = site::host_of(url).unwrap_or_else(|| "unreadable tracker URL".into());
                    *unrecognized.entry(format!("no site has the domain {host}")).or_default() += 1;
                }
            }
            let site_id = site.map(|s| s.id.clone());
            complete.push((t, site_id));
        }
        report.unrecognized = into_reasons(unrecognized);

        let pieces = self.pieces_hashes(task, client.as_ref(), &complete, &mut report).await?;
        let mut preview = Preview {
            source_client_id: source.id.clone(),
            read: report,
            sites: Vec::new(),
            candidates: Vec::new(),
        };

        for (i, site) in targets.iter().enumerate() {
            if task.is_cancelled() {
                break;
            }
            task.progress(&format!("asking {}", site.name), i, targets.len());
            let (site_report, hits) = self.query_site(task, site, &complete, &pieces).await;
            let mut site_report = site_report;
            let mut seen_ids = HashSet::new();
            for (pieces_hash, torrent_id) in hits {
                let holders: Vec<&(TorrentInfo, Option<String>)> = complete
                    .iter()
                    .filter(|(t, _)| pieces.get(&t.hash) == Some(&pieces_hash))
                    .collect();
                if holders.iter().any(|(_, s)| s.as_deref() == Some(site.id.as_str())) {
                    site_report.already_seeding += 1;
                    continue;
                }
                let Some((source_torrent, source_site)) = holders
                    .iter()
                    .copied()
                    .find(|(_, s)| s.is_some())
                    .or_else(|| holders.first().copied())
                else {
                    continue;
                };
                if !seen_ids.insert(torrent_id.clone()) {
                    continue;
                }
                let needs_confirmation = source_site.is_none();
                preview.candidates.push(Candidate {
                    id: preview.candidates.len(),
                    source_hash: source_torrent.hash.clone(),
                    source_name: source_torrent.name.clone(),
                    source_site: source_site.clone(),
                    save_path: source_torrent.save_path.clone(),
                    size: source_torrent.size,
                    pieces_hash,
                    target_site: site.id.clone(),
                    target_torrent_id: torrent_id,
                    evidence: "pieces_equal",
                    needs_confirmation,
                    note: if needs_confirmation {
                        "the local torrent's site is not recognized; it may be this same site".into()
                    } else {
                        "the site has a torrent with identical pieces; file layout is checked after download".into()
                    },
                });
            }
            preview.sites.push(site_report);
        }
        task.progress("done", targets.len(), targets.len());
        Ok(preview)
    }

    pub fn keep_preview(&self, task_id: &str, preview: Preview) -> Arc<Preview> {
        let preview = Arc::new(preview);
        let mut previews = self.previews.lock().expect("preview list poisoned");
        previews.push_back((task_id.to_string(), preview.clone()));
        while previews.len() > KEEP_PREVIEWS {
            previews.pop_front();
        }
        preview
    }

    /// Pieces hash per info hash, from the cache or the client. A torrent whose pieces
    /// cannot be read gets no entry and is reported, never treated as having none.
    async fn pieces_hashes(
        &self,
        task: &Task,
        client: &dyn BitTorrentClient,
        torrents: &[(TorrentInfo, Option<String>)],
        report: &mut ReadReport,
    ) -> anyhow::Result<HashMap<String, String>> {
        let mut known: HashMap<String, String> = {
            let conn = self.db.conn();
            let mut stmt = conn.prepare("SELECT info_hash, pieces_hash FROM piece_hashes")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut missing: BTreeMap<String, usize> = BTreeMap::new();
        let mut result = HashMap::new();
        for (i, (t, _)) in torrents.iter().enumerate() {
            if task.is_cancelled() {
                bail!("cancelled while reading piece hashes");
            }
            if let Some(ph) = known.remove(&t.hash) {
                result.insert(t.hash.clone(), ph);
                continue;
            }
            task.progress("reading piece hashes", i, torrents.len());
            match client.get_piece_hashes(&t.hash).await {
                Ok(hexes) if hexes.is_empty() => *missing.entry("no metadata yet".into()).or_default() += 1,
                Ok(hexes) => match concat_hex(&hexes) {
                    Some(bytes) => {
                        let ph = sha1_hex(&bytes);
                        self.db.conn().execute(
                            "INSERT OR REPLACE INTO piece_hashes (info_hash, pieces_hash) VALUES (?1, ?2)",
                            [&t.hash, &ph],
                        )?;
                        result.insert(t.hash.clone(), ph);
                    }
                    None => *missing.entry("the client returned malformed piece hashes".into()).or_default() += 1,
                },
                Err(ClientError::Unsupported(why)) => bail!("this client cannot be a source: {why}"),
                Err(e) => *missing.entry(format!("reading piece hashes failed: {e}")).or_default() += 1,
            }
        }
        report.without_pieces = into_reasons(missing);
        Ok(result)
    }

    async fn query_site(
        &self,
        task: &Task,
        site: &Site,
        torrents: &[(TorrentInfo, Option<String>)],
        pieces: &HashMap<String, String>,
    ) -> (SiteReport, HashMap<String, String>) {
        let mut report = SiteReport {
            site_id: site.id.clone(),
            queried: 0,
            found: 0,
            already_seeding: 0,
            error: None,
        };
        if site.template_type != TemplateType::NexusPHP {
            report.error = Some(format!("{} sites have no pieces-hash lookup", site.template_type));
            return (report, HashMap::new());
        }
        if site.passkey.is_none() {
            report.error = Some("no passkey is stored for this site".into());
            return (report, HashMap::new());
        }
        // Torrents from the site itself are not looked up there: they are already seeded.
        let hashes: Vec<String> = torrents
            .iter()
            .filter(|(_, s)| s.as_deref() != Some(site.id.as_str()))
            .filter_map(|(t, _)| pieces.get(&t.hash).cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let mut hits = HashMap::new();
        'batches: for batch in hashes.chunks(lookup::MAX_BATCH) {
            // A network failure gets one resend: lookups are read-only and the path to a site can stall.
            for attempt in 1..=2 {
                tokio::select! {
                    _ = self.gates.wait(&site.id, site.rate_limit_rpm) => {}
                    _ = task.cancelled() => break 'batches,
                }
                match lookup::query(&self.http, site, batch).await {
                    Ok(found) => {
                        report.queried += batch.len();
                        hits.extend(found.into_iter().filter(|(h, _)| batch.contains(h)));
                        continue 'batches;
                    }
                    Err(e @ LookupError::Network(_)) if attempt == 1 => {
                        warn!(site = %site.id, "pieces-hash lookup failed, sending it once more: {e}");
                    }
                    Err(e) => {
                        warn!(site = %site.id, "pieces-hash lookup failed: {e}");
                        report.error = Some(e.to_string());
                        break 'batches;
                    }
                }
            }
        }
        report.found = hits.len();
        (report, hits)
    }

    /// Add the confirmed candidates one by one, recording each outcome.
    pub async fn execute(&self, task: &Task, run: ExecuteRun) -> anyhow::Result<ExecuteSummary> {
        let ExecuteRun { candidates, source, target, busy: _busy } = run;
        let run_id = uuid::Uuid::new_v4().to_string();
        let source_client = source.create_client();
        let target_client = target.create_client();
        let mut summary = ExecuteSummary {
            run_id: run_id.clone(),
            success: 0,
            skipped: 0,
            failed: 0,
            not_attempted: 0,
            items: Vec::new(),
        };
        let mut added: HashSet<String> = HashSet::new();

        for (i, candidate) in candidates.iter().enumerate() {
            if task.is_cancelled() {
                summary.not_attempted = candidates.len() - i;
                break;
            }
            task.progress("adding torrents", i, candidates.len());
            let outcome = self
                .execute_one(task, candidate, source_client.as_ref(), target_client.as_ref(), &target.id, &mut added)
                .await;
            if outcome.step == "cancelled" {
                summary.not_attempted = candidates.len() - i;
                break;
            }
            self.db.conn().execute(
                "INSERT INTO reseed_results (run_id, source_hash, source_name, source_site, target_site,
                     target_torrent_id, target_client, target_hash, status, step, message, downloaded)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    run_id,
                    candidate.source_hash,
                    candidate.source_name,
                    candidate.source_site,
                    candidate.target_site,
                    candidate.target_torrent_id,
                    target.id,
                    outcome.target_hash,
                    outcome.status,
                    outcome.step,
                    outcome.message,
                    outcome.downloaded,
                ],
            )?;
            match outcome.status {
                "success" => summary.success += 1,
                "skipped" => summary.skipped += 1,
                _ => summary.failed += 1,
            }
            summary.items.push(ItemResult {
                candidate_id: candidate.id,
                target_site: candidate.target_site.clone(),
                source_name: candidate.source_name.clone(),
                status: outcome.status,
                step: outcome.step,
                message: outcome.message,
            });
        }
        task.progress("done", candidates.len(), candidates.len());
        info!(run = %run_id, success = summary.success, skipped = summary.skipped, failed = summary.failed, "reseed run finished");
        Ok(summary)
    }

    async fn execute_one(
        &self,
        task: &Task,
        candidate: &Candidate,
        source: &dyn BitTorrentClient,
        target: &dyn BitTorrentClient,
        target_id: &str,
        added: &mut HashSet<String>,
    ) -> Outcome {
        let site = match site::load(&self.db.conn(), &candidate.target_site) {
            Ok(Some(s)) if s.enabled => s,
            Ok(_) => return Outcome::new("failed", "site", "the site is no longer enabled"),
            Err(e) => return Outcome::new("failed", "site", e.to_string()),
        };
        // A rerun must not fetch again what an earlier run already added and is still there.
        let earlier: Option<String> = match self.db.conn().query_row(
            "SELECT target_hash FROM reseed_results WHERE status = 'success' AND target_site = ?1
               AND target_torrent_id = ?2 AND target_client = ?3 ORDER BY id DESC LIMIT 1",
            [site.id.as_str(), candidate.target_torrent_id.as_str(), target_id],
            |r| r.get(0),
        ) {
            Ok(h) => h,
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Outcome::new("failed", "exists", e.to_string()),
        };
        if let Some(hash) = earlier {
            if let Ok(true) = target.has_torrent(&hash).await {
                let mut o = Outcome::new("skipped", "exists", "an earlier run added this torrent and it is still there");
                o.target_hash = Some(hash);
                return o;
            }
        }
        let used_today: i64 = match self.db.conn().query_row(
            "SELECT count(*) FROM reseed_results WHERE target_site = ?1 AND downloaded = 1
               AND date(created_at, 'localtime') = date('now', 'localtime')",
            [&site.id],
            |r| r.get(0),
        ) {
            Ok(n) => n,
            Err(e) => return Outcome::new("failed", "daily_limit", e.to_string()),
        };
        if used_today >= i64::from(site.daily_limit) {
            return Outcome::new(
                "skipped",
                "daily_limit",
                format!("the site's daily limit of {} downloads is used up", site.daily_limit),
            );
        }

        tokio::select! {
            _ = self.gates.wait(&site.id, site.rate_limit_rpm) => {}
            _ = task.cancelled() => return Outcome::new("skipped", "cancelled", "cancelled"),
        }
        let downloaded = site.create_template().download_torrent(&self.downloads, &candidate.target_torrent_id).await;
        // A missing credential fails before any request reaches the site.
        let requested = !matches!(
            downloaded,
            Err(site::templates::TemplateError::MissingPasskey | site::templates::TemplateError::MissingAuthkey)
        );
        let mut outcome = self.verify_and_add(candidate, downloaded, source, target, added).await;
        outcome.downloaded = requested;
        outcome
    }

    async fn verify_and_add(
        &self,
        candidate: &Candidate,
        downloaded: Result<Vec<u8>, site::templates::TemplateError>,
        source: &dyn BitTorrentClient,
        target: &dyn BitTorrentClient,
        added: &mut HashSet<String>,
    ) -> Outcome {
        let bytes = match downloaded {
            Ok(b) => b,
            Err(e) => return Outcome::new("failed", "download", e.to_string()),
        };
        let meta = match torrent::parse(&bytes) {
            Ok(m) => m,
            Err(e) => return Outcome::new("failed", "parse", e.to_string()),
        };
        let with_hash = |mut o: Outcome| {
            o.target_hash = Some(meta.info_hash.clone());
            o
        };
        if meta.pieces_hash != candidate.pieces_hash {
            return with_hash(Outcome::new(
                "failed",
                "verify",
                "the downloaded torrent's pieces differ from the lookup result",
            ));
        }
        if added.contains(&meta.info_hash) {
            return with_hash(Outcome::new("skipped", "exists", "already added in this run"));
        }
        match target.has_torrent(&meta.info_hash).await {
            Ok(true) => return with_hash(Outcome::new("skipped", "exists", "the target client already has this torrent")),
            Ok(false) => {}
            Err(e) => return with_hash(Outcome::new("failed", "exists", format!("could not read the target client: {e}"))),
        }

        let on_disk = match source.get_torrent_files(&candidate.source_hash).await {
            Ok(files) => {
                let mut f: Vec<(String, u64)> = files.into_iter().map(|f| (f.name, f.size)).collect();
                f.sort();
                f
            }
            Err(e) => return with_hash(Outcome::new("failed", "layout", format!("could not read the local files: {e}"))),
        };
        let mut wanted = meta.files.clone();
        wanted.sort();
        if wanted != on_disk {
            let first = wanted.iter().zip(&on_disk).find(|(a, b)| a != b).map(|(a, b)| (a.0.clone(), b.0.clone()));
            let detail = match first {
                Some((want, have)) => format!("the torrent expects {want:?}, the disk has {have:?}"),
                None => format!("the torrent has {} files, the disk has {}", wanted.len(), on_disk.len()),
            };
            return with_hash(Outcome::new(
                "skipped",
                "layout",
                format!("same pieces but a different file layout, which needs hard-linking (not supported yet): {detail}"),
            ));
        }

        let options = AddTorrentOptions { save_path: candidate.save_path.clone(), tags: vec![TAG.to_string()] };
        let add_error = match target.add_torrent(&bytes, options).await {
            Ok(()) => None,
            Err(ClientError::Duplicate) => {
                return with_hash(Outcome::new("skipped", "exists", "the target client already has this torrent"))
            }
            Err(e) => Some(e.to_string()),
        };
        // Only the client's own list counts as proof; acceptance can lag the request.
        for attempt in 0..5 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            if let Ok(true) = target.has_torrent(&meta.info_hash).await {
                added.insert(meta.info_hash.clone());
                let mut message = "added stopped; the client checks the data before it seeds".to_string();
                if let Some(e) = &add_error {
                    message.push_str(&format!(". Warning: {e}"));
                }
                return with_hash(Outcome::new("success", "added", message));
            }
        }
        with_hash(Outcome::new(
            "failed",
            "add",
            add_error.unwrap_or_else(|| "the client did not list the torrent after adding it".into()),
        ))
    }
}

fn concat_hex(hexes: &[String]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(hexes.len() * 20);
    for h in hexes {
        if h.len() != 40 {
            return None;
        }
        for pair in h.as_bytes().chunks(2) {
            out.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
        }
    }
    Some(out)
}

fn into_reasons(counts: BTreeMap<String, usize>) -> Vec<Reason> {
    let mut reasons: Vec<Reason> = counts.into_iter().map(|(reason, count)| Reason { reason, count }).collect();
    reasons.sort_by_key(|r| std::cmp::Reverse(r.count));
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_hex_concatenation_matches_raw_pieces() {
        let raw = [0xabu8; 40];
        let hexes = vec!["ab".repeat(20), "ab".repeat(20)];
        assert_eq!(concat_hex(&hexes).unwrap(), raw);
        assert!(concat_hex(&["zz".repeat(20)]).is_none());
        assert!(concat_hex(&["ab".into()]).is_none());
    }
}
