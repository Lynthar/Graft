//! HTTP API layer

mod auth;
mod error;
pub mod handlers;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    Router,
    routing::{get, post},
};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    trace::TraceLayer,
};

use crate::config::Password;
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
    access: Arc<auth::Access>,
}

impl AppState {
    pub fn new(db: Database, password: Option<Password>) -> Self {
        Self {
            reseed: Arc::new(ReseedService::new(db.clone())),
            tasks: Arc::default(),
            access: Arc::new(auth::Access::new(password)),
            db,
        }
    }
}

/// Create the application router
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // Health check
        .route("/health", get(handlers::health))

        // Access
        .route("/auth", get(auth::status))
        .route("/login", post(auth::login))
        .route("/logout", post(auth::logout))

        // Clients
        .route("/clients", get(handlers::client::list).post(handlers::client::create))
        .route("/clients/{id}", get(handlers::client::get_one).put(handlers::client::update).delete(handlers::client::remove))
        .route("/clients/{id}/test", post(handlers::client::test))

        // Sites
        .route("/sites", get(handlers::site::list).post(handlers::site::create))
        .route("/sites/{id}", get(handlers::site::get_one).put(handlers::site::update).delete(handlers::site::remove))

        // Reseed
        .route("/reseed/preview", post(handlers::reseed::preview))
        // Uploaded torrents arrive as base64 JSON: up to 64 MiB in one request.
        .route("/reseed/import", post(handlers::reseed::import).layer(DefaultBodyLimit::max(64 << 20)))
        .route("/reseed/execute", post(handlers::reseed::execute))
        .route("/reseed/history", get(handlers::reseed::history))
        .route("/tasks/{id}", get(handlers::reseed::task_status))
        .route("/tasks/{id}/cancel", post(handlers::reseed::task_cancel))

        // Stats
        .route("/stats", get(handlers::stats))
        .fallback(|| async { AppError::not_found("没有这个接口") });

    Router::new()
        .nest("/api", api_routes)
        // Serve static files
        .fallback(handlers::static_handler)
        .layer(middleware::from_fn_with_state(state.clone(), auth::guard))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
