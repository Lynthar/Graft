//! API request handlers

pub mod client;
pub mod index;
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

/// Health check endpoint
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Dashboard stats
pub async fn stats(
    axum::extract::State(state): axum::extract::State<super::AppState>,
) -> Result<Json<serde_json::Value>, super::AppError> {
    let index_stats = state.index_service.get_stats()?;

    // Get client count
    let client_count: i64 = state.db.conn().query_row(
        "SELECT COUNT(*) FROM clients",
        [],
        |row| row.get(0),
    )?;

    // Get site count
    let site_count: i64 = state.db.conn().query_row(
        "SELECT COUNT(*) FROM sites WHERE enabled = 1",
        [],
        |row| row.get(0),
    )?;

    // Get recent history stats
    let today_success: i64 = state.db.conn().query_row(
        "SELECT COUNT(*) FROM reseed_history WHERE status = 'success' AND date(created_at) = date('now')",
        [],
        |row| row.get(0),
    )?;

    let today_failed: i64 = state.db.conn().query_row(
        "SELECT COUNT(*) FROM reseed_history WHERE status = 'failed' AND date(created_at) = date('now')",
        [],
        |row| row.get(0),
    )?;

    Ok(Json(json!({
        "index": index_stats,
        "clients": client_count,
        "sites": site_count,
        "today": {
            "success": today_success,
            "failed": today_failed,
        }
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
            .body(Body::from("Not Found"))
            .unwrap(),
    }
}
