//! Client management handlers

use axum::{
    extract::{Path, State},
    Json,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::non_blank;
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
    pub link_dir: Option<String>,
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
    #[serde(default)]
    pub link_dir: Option<String>,
}

const VIEW_COLUMNS: &str = "id, name, client_type, host, port, username, use_https, enabled, link_dir";

fn view(row: &rusqlite::Row) -> rusqlite::Result<ClientResponse> {
    Ok(ClientResponse {
        id: row.get(0)?,
        name: row.get(1)?,
        client_type: row.get(2)?,
        host: row.get(3)?,
        port: row.get(4)?,
        username: row.get(5)?,
        use_https: row.get::<_, i32>(6)? != 0,
        enabled: row.get::<_, i32>(7)? != 0,
        link_dir: row.get(8)?,
    })
}

/// Blank clears the directory; anything else must be an absolute path.
fn link_dir(value: Option<String>) -> Result<Option<String>, AppError> {
    let dir = non_blank(value);
    if dir.as_deref().is_some_and(|d| !std::path::Path::new(d).is_absolute()) {
        return Err(AppError::bad_request("硬链接目录要写绝对路径，比如 /downloads/graft-links"));
    }
    Ok(dir)
}

fn load_view(conn: &rusqlite::Connection, id: &str) -> Result<ClientResponse, AppError> {
    let sql = format!("SELECT {VIEW_COLUMNS} FROM clients WHERE id = ?1");
    conn.query_row(&sql, [id], view).optional()?.ok_or_else(|| AppError::not_found("没有这个下载器"))
}

/// List all clients
pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClientResponse>>, AppError> {
    let conn = state.db.conn();
    let mut stmt = conn.prepare(&format!("SELECT {VIEW_COLUMNS} FROM clients ORDER BY name"))?;
    let clients = stmt.query_map([], view)?.collect::<Result<Vec<_>, _>>()?;
    Ok(Json(clients))
}

/// Get a single client
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ClientResponse>, AppError> {
    load_view(&state.db.conn(), &id).map(Json)
}

/// Create a new client
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateClientRequest>,
) -> Result<Json<ClientResponse>, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let link_dir = link_dir(req.link_dir)?;
    let conn = state.db.conn();
    conn.execute(
        "INSERT INTO clients (id, name, client_type, host, port, username, password, use_https, link_dir, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
        rusqlite::params![
            id,
            req.name,
            req.client_type.to_string(),
            req.host,
            req.port,
            req.username,
            non_blank(req.password),
            req.use_https as i32,
            link_dir,
        ],
    )?;
    load_view(&conn, &id).map(Json)
}

/// Update a client. A blank password keeps the stored one, since it is never sent back
/// to be edited; a blank hard-link directory clears it, since it is shown and editable.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateClientRequest>,
) -> Result<Json<ClientResponse>, AppError> {
    let link_dir = link_dir(req.link_dir)?;
    let conn = state.db.conn();
    let rows = conn.execute(
        "UPDATE clients SET name = ?1, client_type = ?2, host = ?3, port = ?4, username = ?5,
             password = COALESCE(?6, password), use_https = ?7, link_dir = ?8, updated_at = datetime('now')
         WHERE id = ?9",
        rusqlite::params![
            req.name,
            req.client_type.to_string(),
            req.host,
            req.port,
            req.username,
            non_blank(req.password),
            req.use_https as i32,
            link_dir,
            id,
        ],
    )?;

    if rows == 0 {
        return Err(AppError::not_found("没有这个下载器"));
    }
    load_view(&conn, &id).map(Json)
}

/// Delete a client
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.conn();
    let rows = conn.execute("DELETE FROM clients WHERE id = ?1", [&id])?;

    if rows == 0 {
        return Err(AppError::not_found("没有这个下载器"));
    }

    Ok(Json(serde_json::json!({"deleted": true})))
}

/// Test client connection
pub async fn test(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = load_client(&state, &id)?;
    let client = config.create_client();

    match client.test_connection().await {
        Ok(true) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "连接成功"
        }))),
        Ok(false) => Ok(Json(serde_json::json!({
            "success": false,
            "message": "连接失败"
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": e.to_string()
        }))),
    }
}

/// Load a client's full configuration, credentials included, for talking to it.
pub(crate) fn load_client(state: &AppState, id: &str) -> Result<ClientConfig, AppError> {
    let conn = state.db.conn();
    let client = conn
        .query_row(
            "SELECT id, name, client_type, host, port, username, password, use_https, link_dir FROM clients WHERE id = ?1",
            [id],
            |row| {
                Ok(ClientConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    client_type: row.get(2)?,
                    host: row.get(3)?,
                    port: row.get(4)?,
                    username: row.get(5)?,
                    password: row.get(6)?,
                    use_https: row.get::<_, i32>(7)? != 0,
                    link_dir: row.get(8)?,
                })
            },
        )
        .optional()?;
    client.ok_or_else(|| AppError::not_found(format!("没有 id 为 {id} 的下载器")))
}
