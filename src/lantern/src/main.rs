//! Lantern — Zen Garden dashboard and service registry daemon
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use garden_lantern::bootstrap::router;
use garden_lantern::tasks::{activity, aggregation, cleanup, discovery};
use garden_lantern::AppState;

#[derive(Parser)]
#[command(name = "lantern")]
#[command(about = "Zen Garden Lantern - Dashboard & service registry daemon")]
struct Cli {
    /// Lantern identifier
    #[arg(long, env = "LANTERN_NAME")]
    lantern_name: Option<String>,

    /// HTTP server port
    #[arg(long, env = "LANTERN_HTTP_PORT")]
    http_port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "RUST_LOG")]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = cli.log_level.unwrap_or_else(|| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .init();

    let lantern_name = cli
        .lantern_name
        .unwrap_or_else(|| "lantern-01".to_string());
    let http_port = cli
        .http_port
        .unwrap_or(garden_common::constants::LANTERN_HTTP);

    tracing::info!(
        lantern_name = %lantern_name,
        http_port = http_port,
        "Lantern daemon starting"
    );

    // Initialize application state
    let state = AppState::new(lantern_name, http_port);

    // Spawn background tasks
    let _ttl_handle = cleanup::spawn_ttl_cleanup(&state);
    let _agg_handle = aggregation::spawn_aggregation(&state);
    let _activity_handle = activity::spawn_activity_collector(&state);
    let _discovery_handle = discovery::spawn_discovery(&state);

    // Build and serve HTTP
    let app = router::configure(state);
    let addr: SocketAddr = format!("0.0.0.0:{}", http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(?addr, "Lantern HTTP server ready");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Lantern daemon shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to install SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to install SIGINT");

        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received"),
            _ = sigint.recv() => tracing::info!("SIGINT received"),
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Ctrl+C received");
    }
}
