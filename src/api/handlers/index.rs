//! Index management handlers

use axum::{
    extract::{Path, State},
    Json,
};

use crate::api::{AppError, AppState};
use crate::client::{ClientConfig, ClientType};
use crate::service::{ImportResult, IndexStats};

/// Get index statistics
pub async fn stats(
    State(state): State<AppState>,
) -> Result<Json<IndexStats>, AppError> {
    let stats = state.index_service.get_stats()?;
    Ok(Json(stats))
}

/// Import torrents from a client
pub async fn import(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Result<Json<ImportResult>, AppError> {
    // Get client config
    let config = get_client_config(&state, &client_id)?;
    let client = config.create_client();

    // Run import
    let result = state.index_service.import_from_client(client.as_ref(), &client_id).await?;

    Ok(Json(result))
}

/// Clear all index entries
pub async fn clear_all(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.index_service.clear()?;
    Ok(Json(serde_json::json!({"cleared": true})))
}

/// Clear index entries for a specific site
pub async fn clear_site(
    State(state): State<AppState>,
    Path(site_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.index_service.clear_by_site(&site_id)?;
    Ok(Json(serde_json::json!({"cleared": true, "site_id": site_id})))
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
