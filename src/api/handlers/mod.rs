//! API request handlers

pub mod client;
pub mod reseed;
pub mod site;

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use rust_embed::Embed;
use serde_json::json;

use super::WebAssets;

/// A submitted text field: blank means none was given.
pub(crate) fn non_blank(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Health check endpoint
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Dashboard stats; "today" is the server's local day.
pub async fn stats(
    axum::extract::State(state): axum::extract::State<super::AppState>,
) -> Result<Json<serde_json::Value>, super::AppError> {
    let conn = state.db.conn();
    let count = |sql: &str| -> rusqlite::Result<i64> { conn.query_row(sql, [], |row| row.get(0)) };
    let today = "date(created_at, 'localtime') = date('now', 'localtime')";

    Ok(Json(json!({
        "clients": count("SELECT COUNT(*) FROM clients")?,
        "sites": count("SELECT COUNT(*) FROM sites WHERE enabled = 1")?,
        "today": {
            "success": count(&format!("SELECT COUNT(*) FROM reseed_results WHERE status = 'success' AND {today}"))?,
            "failed": count(&format!("SELECT COUNT(*) FROM reseed_results WHERE status = 'failed' AND {today}"))?,
        },
        "total_success": count("SELECT COUNT(*) FROM reseed_results WHERE status = 'success'")?,
    })))
}

/// Static file handler for SPA
pub async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact file
    if let Some(content) = <WebAssets as Embed>::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap();
    }

    // Fallback to index.html for SPA routing
    match <WebAssets as Embed>::get("index.html") {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.into_owned()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(
                "Frontend not built: run `npm ci && npm run build` in web/, then rebuild.",
            ))
            .unwrap(),
    }
}
