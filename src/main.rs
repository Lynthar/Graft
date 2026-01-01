//! Graft - A lightweight, self-hosted PT cross-seeding tool
//!
//! Graft helps you automatically cross-seed torrents across multiple PT sites
//! by matching content fingerprints (file size, structure) rather than relying
//! on cloud-based hash matching services.

use anyhow::Result;
use tracing::info;

mod api;
mod client;
mod config;
mod db;
mod service;
mod site;
mod utils;

use api::AppState;
use config::Settings;
use db::Database;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "graft=info,tower_http=info".into()),
        )
        .init();

    info!("Starting Graft v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let settings = Settings::load()?;
    info!("Configuration loaded from {:?}", settings.config_path());

    // Initialize database
    let db = Database::new(&settings.database.path)?;
    db.migrate()?;
    info!("Database initialized at {:?}", settings.database.path);

    // Create application state
    let state = AppState::new(db, settings.clone());

    // Build router
    let app = api::create_router(state);

    // Start server
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
