//! Graft: self-hosted cross-seeding for private trackers. It asks the trackers you belong to,
//! by piece hash, which of your client's contents they carry, and adds the matches you pick.

use std::future::Future;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};

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

/// Docker sends SIGKILL ten seconds after SIGTERM by default; running tasks get most of
/// that to finish the torrent they are on and record it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);

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
    let tasks = state.tasks.clone();

    // Build router
    let app = api::create_router(state);

    // Start server
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://{}", addr);

    let signal = shutdown_signal().context("Cannot listen for shutdown signals")?;
    let stopping = tasks.clone();
    let shutdown = async move {
        signal.await;
        info!("Shutting down");
        stopping.cancel_all();
    };
    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    // Again after the last requests: one of them may have started a task.
    if !tasks.shut_down(SHUTDOWN_GRACE).await {
        warn!("A task was still running after {SHUTDOWN_GRACE:?}; it stops where it is");
    }

    Ok(())
}

/// Resolves on Ctrl-C, or on SIGTERM from `docker stop` and service managers.
#[cfg(unix)]
fn shutdown_signal() -> std::io::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    })
}

/// Resolves on Ctrl-C.
#[cfg(windows)]
fn shutdown_signal() -> std::io::Result<impl Future<Output = ()>> {
    let mut ctrl_c = tokio::signal::windows::ctrl_c()?;
    Ok(async move {
        ctrl_c.recv().await;
    })
}
