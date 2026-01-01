//! Reseed operation handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppError, AppState};
use crate::client::{ClientConfig, ClientType};
use crate::service::{PreviewResult, ReseedRequest, ReseedResult};
use crate::site::SiteConfig;

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub source_client_id: String,
    pub target_site_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub source_client_id: String,
    pub target_client_id: String,
    pub target_site_ids: Vec<String>,
    #[serde(default)]
    pub add_paused: bool,
    #[serde(default)]
    pub skip_checking: bool,
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
    pub info_hash: String,
    pub source_site: Option<String>,
    pub target_site: String,
    pub status: String,
    pub message: Option<String>,
    pub created_at: String,
}

/// Preview reseed matches
pub async fn preview(
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<PreviewResult>, AppError> {
    // Get source client
    let source_config = get_client_config(&state, &req.source_client_id)?;
    let source_client = source_config.create_client();

    // Get target sites
    let sites = get_site_configs(&state, &req.target_site_ids)?;

    // Run preview
    let result = state.reseed_service
        .preview(source_client.as_ref(), &sites)
        .await?;

    Ok(Json(result))
}

/// Execute reseed operation
pub async fn execute(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ReseedResult>, AppError> {
    // Get source client
    let source_config = get_client_config(&state, &req.source_client_id)?;
    let source_client = source_config.create_client();

    // Get target client
    let target_config = get_client_config(&state, &req.target_client_id)?;
    let target_client = target_config.create_client();

    // Get target sites
    let sites = get_site_configs(&state, &req.target_site_ids)?;

    // Build request
    let reseed_req = ReseedRequest {
        task_id: None,
        source_client_id: req.source_client_id,
        target_client_id: req.target_client_id,
        target_site_ids: req.target_site_ids,
        add_paused: req.add_paused,
        skip_checking: req.skip_checking,
    };

    // Execute
    let result = state.reseed_service
        .execute(reseed_req, source_client.as_ref(), target_client.as_ref(), &sites)
        .await?;

    Ok(Json(result))
}

/// Get reseed history
pub async fn history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>, AppError> {
    let conn = state.db.conn();

    let entries = if let Some(ref status) = query.status {
        let sql = "SELECT id, info_hash, source_site, target_site, status, message, created_at
             FROM reseed_history
             WHERE status = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params![status, query.limit, query.offset], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                info_hash: row.get(1)?,
                source_site: row.get(2)?,
                target_site: row.get(3)?,
                status: row.get(4)?,
                message: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    } else {
        let sql = "SELECT id, info_hash, source_site, target_site, status, message, created_at
         FROM reseed_history
         ORDER BY created_at DESC
         LIMIT ?1 OFFSET ?2";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params![query.limit, query.offset], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                info_hash: row.get(1)?,
                source_site: row.get(2)?,
                target_site: row.get(3)?,
                status: row.get(4)?,
                message: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    Ok(Json(entries))
}

/// Helper to get client config from database
fn get_client_config(state: &AppState, id: &str) -> Result<ClientConfig, AppError> {
    let conn = state.db.conn();
    conn.query_row(
        "SELECT id, name, client_type, host, port, username, password_encrypted, use_https FROM clients WHERE id = ?1",
        [id],
        |row| {
            let client_type_str: String = row.get(2)?;
            Ok(ClientConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                client_type: client_type_str.parse().unwrap_or(ClientType::QBittorrent),
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                password: row.get(6)?,
                use_https: row.get::<_, i32>(7)? != 0,
            })
        },
    ).map_err(|_| AppError::not_found("Client not found"))
}

/// Helper to get site configs from database
fn get_site_configs(state: &AppState, site_ids: &[String]) -> Result<Vec<SiteConfig>, AppError> {
    let conn = state.db.conn();
    let mut sites = Vec::new();

    for site_id in site_ids {
        let site = conn.query_row(
            "SELECT id, name, base_url, template_type, passkey, cookie_encrypted, enabled, rate_limit_rpm
             FROM sites WHERE id = ?1 AND enabled = 1",
            [site_id],
            |row| {
                let template_str: String = row.get(3)?;
                Ok(SiteConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_url: row.get(2)?,
                    template_type: template_str.parse().unwrap_or(crate::site::TemplateType::NexusPHP),
                    tracker_domains: Vec::new(), // Not needed for download
                    download_pattern: get_download_pattern(&template_str),
                    passkey: row.get(4)?,
                    cookie: row.get(5)?,
                    enabled: row.get::<_, i32>(6)? != 0,
                    rate_limit_rpm: row.get(7)?,
                })
            },
        );

        if let Ok(site) = site {
            sites.push(site);
        }
    }

    Ok(sites)
}

fn get_download_pattern(template_type: &str) -> String {
    match template_type {
        "unit3d" => "/torrent/download/{id}.{passkey}".to_string(),
        "gazelle" => "/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}".to_string(),
        _ => "/download.php?id={id}&passkey={passkey}".to_string(),
    }
}
