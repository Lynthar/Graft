//! Reseed handlers: preview and execution run as background tasks.

use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::handlers::client::load_client;
use crate::api::{AppError, AppState};
use crate::client::ClientType;
use crate::service::tasks::Snapshot;
use crate::service::ExecuteRun;
use crate::site;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    pub source_client_id: String,
    pub target_site_ids: Vec<String>,
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

#[derive(Debug, Serialize)]
pub struct Started {
    pub task_id: String,
}

pub async fn preview(
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<Started>, AppError> {
    let source = load_client(&state, &req.source_client_id)?;
    if source.client_type != ClientType::QBittorrent {
        return Err(AppError::bad_request(
            "Only qBittorrent can be the source: Transmission does not report piece hashes",
        ));
    }
    if req.target_site_ids.is_empty() {
        return Err(AppError::bad_request("Choose at least one target site"));
    }
    let mut targets = Vec::new();
    {
        let conn = state.db.conn();
        for id in &req.target_site_ids {
            match site::load(&conn, id)? {
                Some(s) if s.enabled => targets.push(s),
                Some(s) => return Err(AppError::bad_request(format!("The site {} ({id}) is not enabled", s.name))),
                None => return Err(AppError::bad_request(format!("No site with id {id}"))),
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

pub async fn execute(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<Started>, AppError> {
    let preview = state.reseed.preview_result(&req.preview_id).ok_or_else(|| {
        AppError::bad_request("That preview is no longer available; run the preview again")
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
            .ok_or_else(|| AppError::bad_request(format!("The preview has no candidate {id}")))?;
        if candidate.needs_confirmation && !req.confirmed_risky_ids.contains(id) {
            return Err(AppError::bad_request(format!(
                "Candidate {id} ({}) needs its own confirmation: {}",
                candidate.source_name, candidate.note
            )));
        }
        candidates.push(candidate.clone());
    }
    if candidates.is_empty() {
        return Err(AppError::bad_request("No candidates were confirmed"));
    }
    let busy = state.reseed.claim_target(&target.id).ok_or_else(|| {
        AppError::new(StatusCode::CONFLICT, format!("A reseed into {} is already running", target.name))
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
    let task = state.tasks.get(&id).ok_or_else(|| AppError::not_found("No such task"))?;
    Ok(Json(task.snapshot()))
}

pub async fn task_cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !state.tasks.cancel(&id) {
        return Err(AppError::not_found("No such task"));
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
