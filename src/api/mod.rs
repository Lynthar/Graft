//! HTTP API layer

mod error;
pub mod handlers;

use axum::{
    Router,
    routing::{get, post},
};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    trace::TraceLayer,
};

use crate::db::Database;
use crate::service::tasks::TaskRegistry;
use crate::service::ReseedService;

pub use error::AppError;

/// Embedded frontend assets; empty when `web/dist` hasn't been built.
#[derive(RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct WebAssets;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub reseed: Arc<ReseedService>,
    pub tasks: Arc<TaskRegistry>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            reseed: Arc::new(ReseedService::new(db.clone())),
            tasks: Arc::default(),
            db,
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

        // Sites
        .route("/sites", get(handlers::site::list).post(handlers::site::create))
        .route("/sites/{id}", get(handlers::site::get_one).put(handlers::site::update).delete(handlers::site::remove))

        // Reseed
        .route("/reseed/preview", post(handlers::reseed::preview))
        .route("/reseed/execute", post(handlers::reseed::execute))
        .route("/reseed/history", get(handlers::reseed::history))
        .route("/tasks/{id}", get(handlers::reseed::task_status))
        .route("/tasks/{id}/cancel", post(handlers::reseed::task_cancel))

        // Stats
        .route("/stats", get(handlers::stats))
        .fallback(|| async { AppError::not_found("Unknown API endpoint") });

    Router::new()
        .nest("/api", api_routes)
        // Serve static files
        .fallback(handlers::static_handler)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
