//! Reseed handlers: preview and execution run as background tasks.

use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::api::handlers::client::load_client;
use crate::api::{AppError, AppState};
use crate::client::{ClientConfig, ClientType};
use crate::service::tasks::Snapshot;
use crate::service::{ExecuteRun, LinkRun};
use crate::site;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    pub source_client_id: String,
    pub target_site_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub source_client_id: String,
    pub files: Vec<UploadedTorrent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadedTorrent {
    pub name: String,
    /// The file's bytes, base64.
    pub data: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    pub preview_id: String,
    pub target_client_id: String,
    /// Candidates confirmed in the preview; nothing else is executed.
    pub candidate_ids: Vec<usize>,
    /// Candidates flagged `needs_confirmation` must also be listed here.
    #[serde(default)]
    pub confirmed_risky_ids: Vec<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkRequest {
    /// The execution that offered the links.
    pub run_id: String,
    /// The offered links the user confirmed, one by one.
    pub candidate_ids: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct Started {
    pub task_id: String,
}

fn load_source(state: &AppState, id: &str) -> Result<ClientConfig, AppError> {
    let source = load_client(state, id)?;
    if source.client_type != ClientType::QBittorrent {
        return Err(AppError::bad_request("只有 qBittorrent 能作来源：Transmission 不提供 piece 哈希"));
    }
    Ok(source)
}

pub async fn preview(
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<Started>, AppError> {
    let source = load_source(&state, &req.source_client_id)?;
    if req.target_site_ids.is_empty() {
        return Err(AppError::bad_request("至少选一个目标站点"));
    }
    let mut targets = Vec::new();
    {
        let conn = state.db.conn();
        for id in &req.target_site_ids {
            match site::load(&conn, id)? {
                Some(s) if s.enabled => targets.push(s),
                Some(s) => return Err(AppError::bad_request(format!("站点 {}（{id}）没有启用", s.name))),
                None => return Err(AppError::bad_request(format!("没有 id 为 {id} 的站点"))),
            }
        }
    }

    let service = state.reseed.clone();
    let task = state.tasks.spawn("preview", move |task| async move {
        let preview = service.preview(&task, source, targets).await?;
        service.keep_preview(&task.id, preview.clone());
        Ok(preview)
    });
    Ok(Json(Started { task_id: task.id.clone() }))
}

/// Match uploaded `.torrent` files against the source client; the result is a preview.
pub async fn import(
    State(state): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<Started>, AppError> {
    let source = load_source(&state, &req.source_client_id)?;
    if req.files.is_empty() {
        return Err(AppError::bad_request("至少选一个种子文件"));
    }
    let files = req
        .files
        .into_iter()
        .map(|f| match base64::engine::general_purpose::STANDARD.decode(&f.data) {
            Ok(bytes) => Ok((f.name, bytes)),
            Err(_) => Err(AppError::bad_request(format!("{} 的内容没能读出来", f.name))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let service = state.reseed.clone();
    let task = state.tasks.spawn("preview", move |task| async move {
        let preview = service.import(&task, source, files).await?;
        service.keep_preview(&task.id, preview.clone());
        Ok(preview)
    });
    Ok(Json(Started { task_id: task.id.clone() }))
}

/// Create the hard links an execution offered and the user confirmed, then add the torrents.
pub async fn link(
    State(state): State<AppState>,
    Json(req): Json<LinkRequest>,
) -> Result<Json<Started>, AppError> {
    let offered = state
        .reseed
        .run_links(&req.run_id)
        .ok_or_else(|| AppError::bad_request("这一轮待确认的硬链接已失效，请重新执行"))?;
    if req.candidate_ids.is_empty() {
        return Err(AppError::bad_request("没有勾选任何条目"));
    }
    let plans = req
        .candidate_ids
        .iter()
        .map(|id| offered.plans.get(id).cloned().ok_or_else(|| AppError::bad_request(format!("这一轮没有要硬链接的候选 {id}"))))
        .collect::<Result<Vec<_>, _>>()?;
    let target = load_client(&state, &offered.target_client_id)?;
    let busy = state.reseed.claim_target(&target.id).ok_or_else(|| {
        AppError::new(StatusCode::CONFLICT, format!("{} 已经有一轮辅种在执行", target.name))
    })?;
    let service = state.reseed.clone();
    let run = LinkRun { plans, target, busy };
    let task = state.tasks.spawn("execute", move |task| async move { service.link(&task, run).await });
    Ok(Json(Started { task_id: task.id.clone() }))
}

pub async fn execute(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<Started>, AppError> {
    let preview = state.reseed.preview_result(&req.preview_id).ok_or_else(|| {
        AppError::bad_request("这次预览已失效，请重新预览")
    })?;
    let source = load_client(&state, &preview.source_client_id)?;
    let target = load_client(&state, &req.target_client_id)?;

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for id in &req.candidate_ids {
        if !seen.insert(*id) {
            continue;
        }
        let candidate = preview
            .candidates
            .iter()
            .find(|c| c.id == *id)
            .ok_or_else(|| AppError::bad_request(format!("预览里没有候选 {id}")))?;
        if candidate.needs_confirmation && !req.confirmed_risky_ids.contains(id) {
            return Err(AppError::bad_request(format!(
                "候选 {id}（{}）需要单独确认：{}",
                candidate.source_name, candidate.note
            )));
        }
        candidates.push(candidate.clone());
    }
    if candidates.is_empty() {
        return Err(AppError::bad_request("没有确认任何候选"));
    }
    let busy = state.reseed.claim_target(&target.id).ok_or_else(|| {
        AppError::new(StatusCode::CONFLICT, format!("{} 已经有一轮辅种在执行", target.name))
    })?;

    let service = state.reseed.clone();
    let run = ExecuteRun { candidates, source, target, busy };
    let task = state.tasks.spawn("execute", move |task| async move { service.execute(&task, run).await });
    Ok(Json(Started { task_id: task.id.clone() }))
}

pub async fn task_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Snapshot>, AppError> {
    let task = state.tasks.get(&id).ok_or_else(|| AppError::not_found("没有这个任务"))?;
    Ok(Json(task.snapshot()))
}

pub async fn task_cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !state.tasks.cancel(&id) {
        return Err(AppError::not_found("没有这个任务"));
    }
    Ok(Json(serde_json::json!({ "cancelling": true })))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub run_id: String,
    pub source_name: String,
    pub source_site: Option<String>,
    pub target_site: String,
    pub target_torrent_id: String,
    pub target_client: String,
    pub status: String,
    pub step: String,
    pub message: String,
    pub created_at: String,
}

pub async fn history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>, AppError> {
    let conn = state.db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, run_id, source_name, source_site, target_site, target_torrent_id, target_client,
                status, step, message, created_at
         FROM reseed_results
         WHERE ?1 IS NULL OR status = ?1
         ORDER BY id DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![query.status, query.limit.clamp(1, 500), query.offset.max(0)],
            |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    source_name: r.get(2)?,
                    source_site: r.get(3)?,
                    target_site: r.get(4)?,
                    target_torrent_id: r.get(5)?,
                    target_client: r.get(6)?,
                    status: r.get(7)?,
                    step: r.get(8)?,
                    message: r.get(9)?,
                    created_at: r.get(10)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}
