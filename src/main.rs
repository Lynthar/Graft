//! Graft: self-hosted cross-seeding for private trackers. It asks the trackers you belong to,
//! by piece hash, which of your client's contents they carry, and adds the matches you pick.

use anyhow::{Context, Result};
use tracing::info;

mod api;
mod client;
mod config;
mod db;
mod service;
mod site;
mod torrent;

use api::AppState;
use config::Settings;
use db::Database;

/// `err` and every cause beneath it, joined by ": ".
pub(crate) fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(err), |e| e.source()).map(ToString::to_string).collect::<Vec<_>>().join(": ")
}

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
    db.migrate()
        .with_context(|| format!("Cannot use database {:?}", settings.database.path))?;
    info!("Database initialized at {:?}", settings.database.path);

    // Create application state
    if settings.server.password.is_some() {
        info!("A password is required to use Graft");
    }
    let state = AppState::new(db, settings.server.password.clone());

    // Build router
    let app = api::create_router(state);

    // Start server
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
