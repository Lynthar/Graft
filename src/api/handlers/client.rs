//! Client management handlers

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppError, AppState};
use crate::client::{ClientConfig, ClientType};

#[derive(Debug, Serialize)]
pub struct ClientResponse {
    pub id: String,
    pub name: String,
    pub client_type: ClientType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub use_https: bool,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    pub client_type: ClientType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub use_https: bool,
}

/// List all clients
pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClientResponse>>, AppError> {
    let conn = state.db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, client_type, host, port, username, use_https, enabled FROM clients ORDER BY name"
    )?;

    let clients = stmt
        .query_map([], |row| {
            let client_type_str: String = row.get(2)?;
            Ok(ClientResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                client_type: client_type_str.parse().unwrap_or(ClientType::QBittorrent),
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                use_https: row.get::<_, i32>(6)? != 0,
                enabled: row.get::<_, i32>(7)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(clients))
}

/// Get a single client
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ClientResponse>, AppError> {
    let conn = state.db.conn();
    let client = conn.query_row(
        "SELECT id, name, client_type, host, port, username, use_https, enabled FROM clients WHERE id = ?1",
        [&id],
        |row| {
            let client_type_str: String = row.get(2)?;
            Ok(ClientResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                client_type: client_type_str.parse().unwrap_or(ClientType::QBittorrent),
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                use_https: row.get::<_, i32>(6)? != 0,
                enabled: row.get::<_, i32>(7)? != 0,
            })
        },
    ).map_err(|_| AppError::not_found("Client not found"))?;

    Ok(Json(client))
}

/// Create a new client
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateClientRequest>,
) -> Result<Json<ClientResponse>, AppError> {
    let id = uuid::Uuid::new_v4().to_string();

    let conn = state.db.conn();
    conn.execute(
        "INSERT INTO clients (id, name, client_type, host, port, username, password_encrypted, use_https, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
        rusqlite::params![
            id,
            req.name,
            req.client_type.to_string(),
            req.host,
            req.port,
            req.username,
            req.password, // TODO: encrypt
            req.use_https as i32,
        ],
    )?;

    Ok(Json(ClientResponse {
        id,
        name: req.name,
        client_type: req.client_type,
        host: req.host,
        port: req.port,
        username: req.username,
        use_https: req.use_https,
        enabled: true,
    }))
}

/// Update a client
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateClientRequest>,
) -> Result<Json<ClientResponse>, AppError> {
    let conn = state.db.conn();

    let rows = conn.execute(
        "UPDATE clients SET name = ?1, client_type = ?2, host = ?3, port = ?4, username = ?5, password_encrypted = ?6, use_https = ?7, updated_at = datetime('now')
         WHERE id = ?8",
        rusqlite::params![
            req.name,
            req.client_type.to_string(),
            req.host,
            req.port,
            req.username,
            req.password,
            req.use_https as i32,
            id,
        ],
    )?;

    if rows == 0 {
        return Err(AppError::not_found("Client not found"));
    }

    Ok(Json(ClientResponse {
        id,
        name: req.name,
        client_type: req.client_type,
        host: req.host,
        port: req.port,
        username: req.username,
        use_https: req.use_https,
        enabled: true,
    }))
}

/// Delete a client
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.conn();
    let rows = conn.execute("DELETE FROM clients WHERE id = ?1", [&id])?;

    if rows == 0 {
        return Err(AppError::not_found("Client not found"));
    }

    Ok(Json(serde_json::json!({"deleted": true})))
}

/// Test client connection
pub async fn test(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = get_client_config(&state, &id)?;
    let client = config.create_client();

    match client.test_connection().await {
        Ok(true) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Connection successful"
        }))),
        Ok(false) => Ok(Json(serde_json::json!({
            "success": false,
            "message": "Connection failed"
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": e.to_string()
        }))),
    }
}

/// Get torrents from a client
pub async fn torrents(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::client::TorrentInfo>>, AppError> {
    let config = get_client_config(&state, &id)?;
    let client = config.create_client();

    let torrents = client.get_torrents().await?;
    Ok(Json(torrents))
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
