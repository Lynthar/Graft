//! Site management handlers

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppError, AppState};
use crate::site::{builtin_sites, SiteConfig, TemplateType};

#[derive(Debug, Serialize)]
pub struct SiteResponse {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub template_type: TemplateType,
    pub has_passkey: bool,
    pub has_cookie: bool,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub id: String,
    pub name: String,
    /// Base URL - optional when using builtin template (template provides default)
    pub base_url: Option<String>,
    #[serde(default)]
    pub template_type: Option<TemplateType>,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
    pub enabled: Option<bool>,
}

/// List all configured sites
pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<SiteResponse>>, AppError> {
    let conn = state.db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, base_url, template_type, passkey, cookie_encrypted, enabled FROM sites ORDER BY name"
    )?;

    let sites = stmt
        .query_map([], |row| {
            let template_str: String = row.get(3)?;
            let passkey: Option<String> = row.get(4)?;
            let cookie: Option<String> = row.get(5)?;
            Ok(SiteResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                base_url: row.get(2)?,
                template_type: template_str.parse().unwrap_or(TemplateType::NexusPHP),
                has_passkey: passkey.is_some(),
                has_cookie: cookie.is_some(),
                enabled: row.get::<_, i32>(6)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(sites))
}

/// Get available site templates (built-in sites)
pub async fn available() -> Json<Vec<SiteConfig>> {
    Json(builtin_sites())
}

/// Get a single site
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SiteResponse>, AppError> {
    let conn = state.db.conn();
    let site = conn.query_row(
        "SELECT id, name, base_url, template_type, passkey, cookie_encrypted, enabled FROM sites WHERE id = ?1",
        [&id],
        |row| {
            let template_str: String = row.get(3)?;
            let passkey: Option<String> = row.get(4)?;
            let cookie: Option<String> = row.get(5)?;
            Ok(SiteResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                base_url: row.get(2)?,
                template_type: template_str.parse().unwrap_or(TemplateType::NexusPHP),
                has_passkey: passkey.is_some(),
                has_cookie: cookie.is_some(),
                enabled: row.get::<_, i32>(6)? != 0,
            })
        },
    ).map_err(|_| AppError::not_found("Site not found"))?;

    Ok(Json(site))
}

/// Create or configure a site
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> Result<Json<SiteResponse>, AppError> {
    // Check if site ID exists in built-in sites
    let builtin = builtin_sites();
    let template = builtin.iter().find(|s| s.id == req.id);

    let (base_url, template_type) = if let Some(t) = template {
        (
            req.base_url.clone().unwrap_or_else(|| t.base_url.clone()),
            t.template_type,
        )
    } else {
        // For custom sites, base_url is required
        let base_url = req.base_url.clone()
            .ok_or_else(|| AppError::bad_request("base_url is required for custom sites"))?;
        (
            base_url,
            req.template_type.unwrap_or(TemplateType::NexusPHP),
        )
    };

    let conn = state.db.conn();

    // Insert or update (upsert)
    conn.execute(
        "INSERT INTO sites (id, name, base_url, template_type, passkey, cookie_encrypted, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            base_url = excluded.base_url,
            passkey = COALESCE(excluded.passkey, passkey),
            cookie_encrypted = COALESCE(excluded.cookie_encrypted, cookie_encrypted),
            updated_at = datetime('now')",
        rusqlite::params![
            req.id,
            req.name,
            base_url,
            template_type.to_string(),
            req.passkey,
            req.cookie,
        ],
    )?;

    // Also register tracker domains if it's a built-in site
    if let Some(t) = template {
        for domain in &t.tracker_domains {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO tracker_domains (domain, site_id) VALUES (?1, ?2)",
                [domain, &req.id],
            );
        }
    }

    Ok(Json(SiteResponse {
        id: req.id,
        name: req.name,
        base_url,
        template_type,
        has_passkey: req.passkey.is_some(),
        has_cookie: req.cookie.is_some(),
        enabled: true,
    }))
}

/// Update a site
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSiteRequest>,
) -> Result<Json<SiteResponse>, AppError> {
    // Build and execute update in a scope to drop conn before calling get_one
    {
        let conn = state.db.conn();

        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref name) = req.name {
            updates.push("name = ?");
            params.push(Box::new(name.clone()));
        }
        if let Some(ref base_url) = req.base_url {
            updates.push("base_url = ?");
            params.push(Box::new(base_url.clone()));
        }
        if let Some(ref passkey) = req.passkey {
            updates.push("passkey = ?");
            params.push(Box::new(passkey.clone()));
        }
        if let Some(ref cookie) = req.cookie {
            updates.push("cookie_encrypted = ?");
            params.push(Box::new(cookie.clone()));
        }
        if let Some(enabled) = req.enabled {
            updates.push("enabled = ?");
            params.push(Box::new(enabled as i32));
        }

        if updates.is_empty() {
            return Err(AppError::bad_request("No fields to update"));
        }

        updates.push("updated_at = datetime('now')");
        params.push(Box::new(id.clone()));

        let sql = format!(
            "UPDATE sites SET {} WHERE id = ?",
            updates.join(", ")
        );

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = conn.execute(&sql, params_refs.as_slice())?;

        if rows == 0 {
            return Err(AppError::not_found("Site not found"));
        }
    } // conn is dropped here

    // Fetch updated site
    get_one(State(state), Path(id)).await
}

/// Delete a site
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.conn();
    let rows = conn.execute("DELETE FROM sites WHERE id = ?1", [&id])?;

    if rows == 0 {
        return Err(AppError::not_found("Site not found"));
    }

    // Also remove tracker domain mappings
    conn.execute("DELETE FROM tracker_domains WHERE site_id = ?1", [&id])?;

    Ok(Json(serde_json::json!({"deleted": true})))
}
