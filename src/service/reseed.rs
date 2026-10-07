//! Reseeding: find the source client's content on target sites by pieces hash, then add
//! the matching torrents to a target client, stopped, after verifying what was downloaded.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
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
    /// One entry per uploaded `.torrent`; empty for a preview that asked sites.
    pub imports: Vec<ImportReport>,
    pub candidates: Vec<Candidate>,
}

/// What became of one uploaded `.torrent`.
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub file: String,
    pub site: Option<String>,
    /// `candidate`, `seeding`, `partial`, `none` or `invalid`.
    pub outcome: &'static str,
    pub detail: String,
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
    /// Empty when an uploaded torrent's tracker belongs to no configured site.
    pub target_site: String,
    /// Empty for an uploaded torrent, which names no torrent id on its site.
    pub target_torrent_id: String,
    /// `pieces_equal`: same `info.pieces` as the local torrent. `files_equal`: different
    /// pieces, same file paths and sizes; only the client's check can confirm it.
    pub evidence: &'static str,
    /// Must be confirmed one by one before execution may include it.
    pub needs_confirmation: bool,
    pub note: String,
    /// The uploaded `.torrent`; execution adds it without asking its site for anything.
    #[serde(skip)]
    pub torrent: Option<Arc<Vec<u8>>>,
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
    links: Mutex<VecDeque<(String, Arc<RunLinks>)>>,
    busy: Arc<Mutex<HashSet<String>>>,
}

/// Hard links offered by one execution, waiting for the user to confirm them.
pub struct RunLinks {
    pub target_client_id: String,
    pub plans: HashMap<usize, LinkPlan>,
}

/// One torrent whose files are named differently from the local ones holding its bytes.
#[derive(Clone)]
pub struct LinkPlan {
    candidate: Candidate,
    bytes: Arc<Vec<u8>>,
    info_hash: String,
    /// Save path for the torrent: `<link_dir>/<site>`.
    root: PathBuf,
    /// (local file, link to create), in torrent order.
    links: Vec<(PathBuf, PathBuf)>,
}

/// Confirmed hard links to create, validated by the caller.
pub struct LinkRun {
    pub plans: Vec<LinkPlan>,
    pub target: ClientConfig,
    pub busy: BusyGuard,
}

/// What one execution carries from torrent to torrent.
struct RunContext<'a> {
    source: &'a dyn BitTorrentClient,
    target: &'a dyn BitTorrentClient,
    target_id: &'a str,
    link_dir: Option<&'a str>,
    added: HashSet<String>,
    links: HashMap<usize, LinkPlan>,
}

enum Added {
    Yes { warning: Option<String> },
    AlreadyThere,
    No(String),
}

impl ExecuteSummary {
    fn new() -> Self {
        ExecuteSummary {
            run_id: uuid::Uuid::new_v4().to_string(),
            success: 0,
            skipped: 0,
            failed: 0,
            not_attempted: 0,
            items: Vec::new(),
        }
    }
}

struct SourceScan {
    client: Box<dyn BitTorrentClient>,
    sites: Vec<Site>,
    report: ReadReport,
    /// Complete torrents with the site their tracker belongs to.
    complete: Vec<(TorrentInfo, Option<String>)>,
    /// Pieces hash per info hash; torrents whose pieces could not be read have none.
    pieces: HashMap<String, String>,
}

/// How an uploaded torrent relates to the source client's torrents.
enum Upload<'a> {
    Seeding(String),
    Candidate { local: &'a (TorrentInfo, Option<String>), evidence: &'static str, needs_confirmation: bool, note: String },
    Partial(String),
    NoMatch,
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
            links: Mutex::new(VecDeque::new()),
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

    /// Read the source client: its complete torrents, the site each belongs to, and
    /// their pieces hashes where those can be read.
    async fn scan_source(&self, task: &Task, source: &ClientConfig) -> anyhow::Result<SourceScan> {
        let client = source.create_client();
        task.progress("读取来源下载器", 0, 0);
        let torrents = client.get_torrents().await.context("读不了来源下载器")?;
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
                (None, None) => *unrecognized.entry("没有可用的 tracker".into()).or_default() += 1,
                (None, Some(url)) => {
                    let host = site::host_of(url).unwrap_or_else(|| "无法解析的 tracker 地址".into());
                    *unrecognized.entry(format!("没有站点认领域名 {host}")).or_default() += 1;
                }
            }
            let site_id = site.map(|s| s.id.clone());
            complete.push((t, site_id));
        }
        report.unrecognized = into_reasons(unrecognized);

        let pieces = self.pieces_hashes(task, client.as_ref(), &complete, &mut report).await?;
        Ok(SourceScan { client, sites: all_sites, report, complete, pieces })
    }

    /// Read the source client, then ask every target site which contents it carries.
    pub async fn preview(&self, task: &Task, source: ClientConfig, targets: Vec<Site>) -> anyhow::Result<Preview> {
        let SourceScan { report, complete, pieces, .. } = self.scan_source(task, &source).await?;
        let mut preview = Preview {
            source_client_id: source.id.clone(),
            read: report,
            sites: Vec::new(),
            imports: Vec::new(),
            candidates: Vec::new(),
        };

        for (i, site) in targets.iter().enumerate() {
            if task.is_cancelled() {
                break;
            }
            task.progress(&format!("查询 {}", site.name), i, targets.len());
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
                        "认不出本地种子来自哪个站，可能就是这个站本身".into()
                    } else {
                        "站点上有 pieces 完全相同的种子，下载后再核对文件布局".into()
                    },
                    torrent: None,
                });
            }
            preview.sites.push(site_report);
        }
        task.progress("完成", targets.len(), targets.len());
        Ok(preview)
    }

    /// Match uploaded `.torrent` files against the source client's complete torrents.
    pub async fn import(&self, task: &Task, source: ClientConfig, files: Vec<(String, Vec<u8>)>) -> anyhow::Result<Preview> {
        let scan = self.scan_source(task, &source).await?;
        let mut preview = Preview {
            source_client_id: source.id.clone(),
            read: scan.report.clone(),
            sites: Vec::new(),
            imports: Vec::new(),
            candidates: Vec::new(),
        };
        let mut file_lists = HashMap::new();
        let total = files.len();
        for (i, (file, bytes)) in files.into_iter().enumerate() {
            if task.is_cancelled() {
                break;
            }
            task.progress("比对上传的种子", i, total);
            let meta = match torrent::parse(&bytes) {
                Ok(m) => m,
                Err(e) => {
                    let detail = e.to_string();
                    preview.imports.push(ImportReport { file, site: None, outcome: "invalid", detail });
                    continue;
                }
            };
            let site = meta.tracker_host.as_deref().and_then(|h| site::recognize_host(&scan.sites, h)).map(|s| s.id.clone());
            let (outcome, detail) = match match_upload(&scan, &meta, site.as_deref(), &mut file_lists).await {
                Upload::Seeding(why) => ("seeding", why),
                Upload::Partial(why) => ("partial", why),
                Upload::NoMatch => ("none", "本地没有找到对应的数据".to_string()),
                Upload::Candidate { local: (t, source_site), evidence, needs_confirmation, note } => {
                    preview.candidates.push(Candidate {
                        id: preview.candidates.len(),
                        source_hash: t.hash.clone(),
                        source_name: t.name.clone(),
                        source_site: source_site.clone(),
                        save_path: t.save_path.clone(),
                        size: t.size,
                        pieces_hash: meta.pieces_hash.clone(),
                        target_site: site.clone().unwrap_or_default(),
                        target_torrent_id: String::new(),
                        evidence,
                        needs_confirmation,
                        note: note.clone(),
                        torrent: Some(Arc::new(bytes)),
                    });
                    ("candidate", note)
                }
            };
            preview.imports.push(ImportReport { file, site, outcome, detail });
        }
        task.progress("完成", total, total);
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
                bail!("读取 piece 哈希时被取消");
            }
            if let Some(ph) = known.remove(&t.hash) {
                result.insert(t.hash.clone(), ph);
                continue;
            }
            task.progress("读取 piece 哈希", i, torrents.len());
            match client.get_piece_hashes(&t.hash).await {
                Ok(hexes) if hexes.is_empty() => *missing.entry("还没有元数据".into()).or_default() += 1,
                Ok(hexes) => match concat_hex(&hexes) {
                    Some(bytes) => {
                        let ph = sha1_hex(&bytes);
                        self.db.conn().execute(
                            "INSERT OR REPLACE INTO piece_hashes (info_hash, pieces_hash) VALUES (?1, ?2)",
                            [&t.hash, &ph],
                        )?;
                        result.insert(t.hash.clone(), ph);
                    }
                    None => *missing.entry("下载器返回的 piece 哈希格式不对".into()).or_default() += 1,
                },
                Err(ClientError::Unsupported(why)) => bail!("这个下载器不能作来源：{why}"),
                Err(e) => *missing.entry(format!("读取 piece 哈希失败：{e}")).or_default() += 1,
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
            report.error = Some("这个站没有填 passkey".into());
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
        let source_client = source.create_client();
        let target_client = target.create_client();
        let mut ctx = RunContext {
            source: source_client.as_ref(),
            target: target_client.as_ref(),
            target_id: &target.id,
            link_dir: target.link_dir.as_deref(),
            added: HashSet::new(),
            links: HashMap::new(),
        };
        let mut summary = ExecuteSummary::new();
        for (i, candidate) in candidates.iter().enumerate() {
            if task.is_cancelled() {
                summary.not_attempted = candidates.len() - i;
                break;
            }
            task.progress("加种", i, candidates.len());
            let outcome = self.execute_one(task, candidate, &mut ctx).await;
            if outcome.step == "cancelled" {
                summary.not_attempted = candidates.len() - i;
                break;
            }
            self.record(&mut summary, candidate, &target.id, outcome)?;
        }
        task.progress("完成", candidates.len(), candidates.len());
        if !ctx.links.is_empty() {
            self.keep_links(&summary.run_id, RunLinks { target_client_id: target.id.clone(), plans: ctx.links });
        }
        info!(run = %summary.run_id, success = summary.success, skipped = summary.skipped, failed = summary.failed, "reseed run finished");
        Ok(summary)
    }

    /// Hard-link and add the torrents of an earlier run that the user confirmed one by one.
    pub async fn link(&self, task: &Task, run: LinkRun) -> anyhow::Result<ExecuteSummary> {
        let LinkRun { plans, target, busy: _busy } = run;
        let client = target.create_client();
        let mut summary = ExecuteSummary::new();
        for (i, plan) in plans.iter().enumerate() {
            if task.is_cancelled() {
                summary.not_attempted = plans.len() - i;
                break;
            }
            task.progress("建硬链接", i, plans.len());
            let mut outcome = link_and_add(client.as_ref(), plan).await;
            outcome.target_hash = Some(plan.info_hash.clone());
            self.record(&mut summary, &plan.candidate, &target.id, outcome)?;
        }
        task.progress("完成", plans.len(), plans.len());
        Ok(summary)
    }

    /// Write one result to the history and count it in `summary`.
    fn record(&self, summary: &mut ExecuteSummary, candidate: &Candidate, target_id: &str, outcome: Outcome) -> anyhow::Result<()> {
        self.db.conn().execute(
            "INSERT INTO reseed_results (run_id, source_hash, source_name, source_site, target_site,
                 target_torrent_id, target_client, target_hash, status, step, message, downloaded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                summary.run_id,
                candidate.source_hash,
                candidate.source_name,
                candidate.source_site,
                candidate.target_site,
                candidate.target_torrent_id,
                target_id,
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
        Ok(())
    }

    fn keep_links(&self, run_id: &str, links: RunLinks) {
        let mut kept = self.links.lock().expect("link list poisoned");
        kept.push_back((run_id.to_string(), Arc::new(links)));
        while kept.len() > KEEP_PREVIEWS {
            kept.pop_front();
        }
    }

    /// The hard links an execution offered, while it is among the most recent ones.
    pub fn run_links(&self, run_id: &str) -> Option<Arc<RunLinks>> {
        let kept = self.links.lock().expect("link list poisoned");
        kept.iter().find(|(id, _)| id == run_id).map(|(_, l)| l.clone())
    }

    async fn execute_one(&self, task: &Task, candidate: &Candidate, ctx: &mut RunContext<'_>) -> Outcome {
        if let Some(bytes) = &candidate.torrent {
            // An uploaded torrent needs nothing from its site: no download, no daily limit.
            return self.verify_and_add(candidate, Ok(bytes.to_vec()), ctx).await;
        }
        let site = match site::load(&self.db.conn(), &candidate.target_site) {
            Ok(Some(s)) if s.enabled => s,
            Ok(_) => return Outcome::new("failed", "site", "站点已停用"),
            Err(e) => return Outcome::new("failed", "site", e.to_string()),
        };
        // A rerun must not fetch again what an earlier run already added and is still there.
        let earlier: Option<String> = match self.db.conn().query_row(
            "SELECT target_hash FROM reseed_results WHERE status = 'success' AND target_site = ?1
               AND target_torrent_id = ?2 AND target_client = ?3 ORDER BY id DESC LIMIT 1",
            [site.id.as_str(), candidate.target_torrent_id.as_str(), ctx.target_id],
            |r| r.get(0),
        ) {
            Ok(h) => h,
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Outcome::new("failed", "exists", e.to_string()),
        };
        if let Some(hash) = earlier {
            if let Ok(true) = ctx.target.has_torrent(&hash).await {
                let mut o = Outcome::new("skipped", "exists", "之前的执行已经加过，下载器里还在");
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
                format!("站点今天的 {} 次下载额度已用完", site.daily_limit),
            );
        }

        tokio::select! {
            _ = self.gates.wait(&site.id, site.rate_limit_rpm) => {}
            _ = task.cancelled() => return Outcome::new("skipped", "cancelled", "已取消"),
        }
        let downloaded = site.create_template().download_torrent(&self.downloads, &candidate.target_torrent_id).await;
        // A missing credential fails before any request reaches the site.
        let requested = !matches!(
            downloaded,
            Err(site::templates::TemplateError::MissingPasskey | site::templates::TemplateError::MissingAuthkey)
        );
        let mut outcome = self.verify_and_add(candidate, downloaded, ctx).await;
        outcome.downloaded = requested;
        outcome
    }

    async fn verify_and_add(
        &self,
        candidate: &Candidate,
        downloaded: Result<Vec<u8>, site::templates::TemplateError>,
        ctx: &mut RunContext<'_>,
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
                "下载到的种子 pieces 与查询结果不符",
            ));
        }
        if ctx.added.contains(&meta.info_hash) {
            return with_hash(Outcome::new("skipped", "exists", "本轮已经加过"));
        }
        match ctx.target.has_torrent(&meta.info_hash).await {
            Ok(true) => return with_hash(Outcome::new("skipped", "exists", "目标下载器里已经有这个种子")),
            Ok(false) => {}
            Err(e) => return with_hash(Outcome::new("failed", "exists", format!("读不了目标下载器：{e}"))),
        }

        // In torrent order: with equal pieces, the i-th files of both hold the same bytes.
        let local: Vec<(String, u64)> = match ctx.source.get_torrent_files(&candidate.source_hash).await {
            Ok(files) => files.into_iter().map(|f| (f.name, f.size)).collect(),
            Err(e) => return with_hash(Outcome::new("failed", "layout", format!("读不了本地的文件列表：{e}"))),
        };
        let (mut on_disk, mut wanted) = (local.clone(), meta.files.clone());
        on_disk.sort();
        wanted.sort();
        if wanted != on_disk {
            return with_hash(match plan_links(candidate, &meta, &local, ctx.link_dir) {
                Err(why) => Outcome::new("skipped", "layout", why),
                Ok(plan) => {
                    let (have, want) = (&local[0].0, &meta.files[0].0);
                    let message = format!(
                        "文件名与本地不同，可以在 {} 建 {} 个硬链接加入（如 {want} ← {have}），源文件不动；在执行结果里勾选确认",
                        plan.0.display(),
                        plan.1.len(),
                    );
                    let (root, links) = plan;
                    let info_hash = meta.info_hash.clone();
                    let plan = LinkPlan { candidate: candidate.clone(), bytes: Arc::new(bytes), info_hash, root, links };
                    ctx.links.insert(candidate.id, plan);
                    Outcome::new("skipped", "link", message)
                }
            });
        }

        with_hash(match add_confirmed(ctx.target, &bytes, &meta.info_hash, candidate.save_path.clone()).await {
            Added::Yes { warning } => {
                ctx.added.insert(meta.info_hash.clone());
                Outcome::new("success", "added", with_warning("已暂停加入，下载器校验数据后才会做种".into(), warning))
            }
            Added::AlreadyThere => Outcome::new("skipped", "exists", "目标下载器里已经有这个种子"),
            Added::No(why) => Outcome::new("failed", "add", why),
        })
    }
}

/// Add stopped, then wait for the client to list it: its own list is the only proof,
/// and acceptance can lag the request.
async fn add_confirmed(target: &dyn BitTorrentClient, bytes: &[u8], info_hash: &str, save_path: String) -> Added {
    let options = AddTorrentOptions { save_path, tags: vec![TAG.to_string()] };
    let add_error = match target.add_torrent(bytes, options).await {
        Ok(()) => None,
        Err(ClientError::Duplicate) => return Added::AlreadyThere,
        Err(e) => Some(e.to_string()),
    };
    for attempt in 0..5 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if let Ok(true) = target.has_torrent(info_hash).await {
            return Added::Yes { warning: add_error };
        }
    }
    Added::No(add_error.unwrap_or_else(|| "加入后下载器的列表里没有这个种子".into()))
}

fn with_warning(mut message: String, warning: Option<String>) -> String {
    if let Some(w) = warning {
        message.push_str(&format!("。警告：{w}"));
    }
    message
}

async fn link_and_add(target: &dyn BitTorrentClient, plan: &LinkPlan) -> Outcome {
    match target.has_torrent(&plan.info_hash).await {
        Ok(true) => return Outcome::new("skipped", "exists", "目标下载器里已经有这个种子"),
        Ok(false) => {}
        Err(e) => return Outcome::new("failed", "exists", format!("读不了目标下载器：{e}")),
    }
    let created = match make_links(&plan.links) {
        Ok(created) => created,
        Err(why) => return Outcome::new("failed", "link", why),
    };
    let save_path = plan.root.to_string_lossy().into_owned();
    match add_confirmed(target, &plan.bytes, &plan.info_hash, save_path).await {
        Added::Yes { warning } => {
            let message = format!("已在 {} 建好硬链接并暂停加入，下载器校验数据后才会做种", plan.root.display());
            Outcome::new("success", "link", with_warning(message, warning))
        }
        Added::AlreadyThere => {
            remove_links(&created);
            Outcome::new("skipped", "exists", "目标下载器里已经有这个种子")
        }
        Added::No(why) => {
            remove_links(&created);
            Outcome::new("failed", "link", why)
        }
    }
}

/// Pair each file of the torrent with the local file holding the same bytes, as a link
/// under `<link_dir>/<site>`. Returns that directory and the (local file, link) pairs.
fn plan_links(
    candidate: &Candidate,
    meta: &torrent::Metainfo,
    local: &[(String, u64)],
    link_dir: Option<&str>,
) -> Result<(PathBuf, Vec<(PathBuf, PathBuf)>), String> {
    let same_split = local.len() == meta.files.len() && local.iter().zip(&meta.files).all(|(a, b)| a.1 == b.1);
    if !same_split {
        return Err(format!(
            "pieces 相同但文件切分不同（种子里 {} 个文件，本地 {} 个），没法用硬链接",
            meta.files.len(),
            local.len()
        ));
    }
    let Some(dir) = link_dir else {
        return Err("文件名与本地不同，可以用硬链接辅种：先在下载器设置里填「硬链接目录」".into());
    };
    let site = if candidate.target_site.is_empty() { "upload" } else { candidate.target_site.as_str() };
    let root = Path::new(dir).join(site);
    let save_path = Path::new(&candidate.save_path);
    // Links must never land inside the local torrent's own folder (or on its single file).
    let local_root = local.first().and_then(|(p, _)| p.split('/').next()).map(|top| save_path.join(top));
    if local_root.is_some_and(|top| root.starts_with(&top)) {
        return Err(format!("硬链接目录 {} 落在本地种子自己的目录里，换一个目录", root.display()));
    }
    let links = local
        .iter()
        .zip(&meta.files)
        .map(|((have, _), (want, _))| {
            let rel = safe_relative(want).ok_or_else(|| format!("种子里的路径不安全，不建链接：{want:?}"))?;
            Ok((save_path.join(have), root.join(rel)))
        })
        .collect::<Result<_, String>>()?;
    Ok((root, links))
}

/// `path` from a torrent as a relative path, or `None` if any part could step outside the
/// directory it is joined to: `..`, an empty part, a backslash, NUL, or a drive or root.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let parts_ok = path.split('/').all(|p| !p.is_empty() && !p.contains(['\\', '\0']));
    let rel: PathBuf = path.split('/').collect();
    let normal = rel.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    (parts_ok && normal).then_some(rel)
}

/// Create every link; one that already points at the same file is kept as it is. On
/// failure the links made here are removed again. Returns the links it created.
fn make_links(links: &[(PathBuf, PathBuf)]) -> Result<Vec<PathBuf>, String> {
    let mut created = Vec::new();
    for (src, dst) in links {
        if let Err(why) = link_file(src, dst, &mut created) {
            remove_links(&created);
            return Err(why);
        }
    }
    Ok(created)
}

fn remove_links(created: &[PathBuf]) {
    for link in created {
        if let Err(e) = std::fs::remove_file(link) {
            warn!(link = %link.display(), "could not remove a hard link made for a failed add: {e}");
        }
    }
}

fn link_file(src: &Path, dst: &Path, created: &mut Vec<PathBuf>) -> Result<(), String> {
    let source = std::fs::metadata(src).map_err(|e| match e.kind() {
        ErrorKind::NotFound => {
            format!("Graft 看不到源文件 {}：容器要按下载器看到的同一路径挂载数据", src.display())
        }
        _ => format!("读不了源文件 {}：{e}", src.display()),
    })?;
    match std::fs::symlink_metadata(dst) {
        Ok(existing) if same_file(&existing, &source) => return Ok(()),
        Ok(_) => return Err(format!("{} 已经有别的文件，不覆盖", dst.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("读不了 {}：{e}", dst.display())),
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建不了目录 {}：{e}", dir.display()))?;
    }
    std::fs::hard_link(src, dst).map_err(|e| match e.kind() {
        ErrorKind::CrossesDevices => {
            format!("硬链接目录与 {} 不在同一个文件系统（ZFS 数据集），链不过去", src.display())
        }
        ErrorKind::PermissionDenied => format!(
            "没有权限给 {} 建硬链接：Linux 只允许给自己拥有、或自己能读写的文件建硬链接，\
             Graft 的 PUID / PGID 要和下载器运行的用户一致",
            src.display()
        ),
        _ => format!("建硬链接失败（{} → {}）：{e}", src.display(), dst.display()),
    })?;
    created.push(dst.to_path_buf());
    Ok(())
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    false
}

/// Same pieces beats same files; a same-named torrent with other files is only reported,
/// since adding it would make the client download into data that is already there.
async fn match_upload<'a>(
    scan: &'a SourceScan,
    meta: &torrent::Metainfo,
    site: Option<&str>,
    file_lists: &mut HashMap<String, Option<Vec<(String, u64)>>>,
) -> Upload<'a> {
    if scan.complete.iter().any(|(t, _)| t.hash.eq_ignore_ascii_case(&meta.info_hash)) {
        return Upload::Seeding("下载器里已经有这个种子".into());
    }
    let holders: Vec<_> = scan.complete.iter().filter(|(t, _)| scan.pieces.get(&t.hash) == Some(&meta.pieces_hash)).collect();
    if let Some(site) = site {
        if holders.iter().any(|(_, s)| s.as_deref() == Some(site)) {
            return Upload::Seeding(format!("已经在 {site} 做种 pieces 相同的种子"));
        }
    }
    if let Some(local) = holders.iter().find(|(_, s)| s.is_some()).or_else(|| holders.first()) {
        let needs_confirmation = local.1.is_none();
        let note = if needs_confirmation {
            "认不出本地种子来自哪个站，可能就是同一个站".into()
        } else {
            format!("与本地「{}」的 pieces 完全相同", local.0.name)
        };
        return Upload::Candidate { local, evidence: "pieces_equal", needs_confirmation, note };
    }

    let mut wanted = meta.files.clone();
    wanted.sort();
    let total: u64 = wanted.iter().map(|(_, len)| len).sum();
    for local in scan.complete.iter().filter(|(t, _)| t.size == total) {
        if local_files(scan, &local.0.hash, file_lists).await == Some(&wanted) {
            let note = format!(
                "pieces 不同（多半是 piece 大小不同），但文件路径与大小都和本地「{}」一致；\
                 加入后以下载器校验为准，校验不到 100% 不要开始",
                local.0.name
            );
            return Upload::Candidate { local, evidence: "files_equal", needs_confirmation: true, note };
        }
    }
    for (t, _) in scan.complete.iter().filter(|(t, _)| t.name == meta.name) {
        let Some(have) = local_files(scan, &t.hash, file_lists).await else { continue };
        let missing: Vec<_> = wanted.iter().filter(|f| !have.contains(f)).collect();
        let extra = have.iter().filter(|f| !wanted.contains(f)).count();
        let example = missing.first().map(|(path, _)| format!("，比如 {path}")).unwrap_or_default();
        return Upload::Partial(format!(
            "与本地同名种子的文件对不上：缺 {} 个、多 {extra} 个{example}；补下载暂不支持",
            missing.len()
        ));
    }
    Upload::NoMatch
}

/// A local torrent's files, sorted, read once per import; `None` when the client fails.
async fn local_files<'m>(
    scan: &SourceScan,
    hash: &str,
    cache: &'m mut HashMap<String, Option<Vec<(String, u64)>>>,
) -> Option<&'m Vec<(String, u64)>> {
    if !cache.contains_key(hash) {
        let files = scan.client.get_torrent_files(hash).await.ok().map(|files| {
            let mut f: Vec<_> = files.into_iter().map(|f| (f.name, f.size)).collect();
            f.sort();
            f
        });
        cache.insert(hash.to_string(), files);
    }
    cache.get(hash).and_then(Option::as_ref)
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
