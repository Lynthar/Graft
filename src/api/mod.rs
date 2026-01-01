//! HTTP API layer

mod error;
pub mod handlers;

use axum::{
    Router,
    routing::{get, post, put, delete},
};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer,
    compression::CompressionLayer,
    trace::TraceLayer,
};

use crate::config::Settings;
use crate::db::Database;
use crate::service::{IndexService, ReseedService};

pub use error::AppError;

/// Embedded frontend assets
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebAssets;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub settings: Settings,
    pub index_service: Arc<IndexService>,
    pub reseed_service: Arc<ReseedService>,
}

impl AppState {
    pub fn new(db: Database, settings: Settings) -> Self {
        let index_service = Arc::new(IndexService::new(db.clone()));
        let reseed_service = Arc::new(ReseedService::new(
            db.clone(),
            index_service.clone(),
        ));

        Self {
            db,
            settings,
            index_service,
            reseed_service,
        }
    }
}

/// Create the application router
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // Health check
        .route("/health", get(handlers::health))

        // Clients
        .route("/clients", get(handlers::client::list).post(handlers::client::create))
        .route("/clients/{id}", get(handlers::client::get_one).put(handlers::client::update).delete(handlers::client::remove))
        .route("/clients/{id}/test", post(handlers::client::test))
        .route("/clients/{id}/torrents", get(handlers::client::torrents))

        // Sites
        .route("/sites", get(handlers::site::list).post(handlers::site::create))
        .route("/sites/available", get(handlers::site::available))
        .route("/sites/{id}", get(handlers::site::get_one).put(handlers::site::update).delete(handlers::site::remove))

        // Index
        .route("/index/stats", get(handlers::index::stats))
        .route("/index/import/{client_id}", post(handlers::index::import))
        .route("/index", delete(handlers::index::clear_all))
        .route("/index/{site_id}", delete(handlers::index::clear_site))

        // Reseed
        .route("/reseed/preview", post(handlers::reseed::preview))
        .route("/reseed/execute", post(handlers::reseed::execute))
        .route("/reseed/history", get(handlers::reseed::history))

        // Stats
        .route("/stats", get(handlers::stats));

    Router::new()
        .nest("/api", api_routes)
        // Serve static files
        .fallback(handlers::static_handler)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
