//! AI Orchestrator (zen-garden.ai.orchestrator)
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.
//! All business logic lives in `domain/`, offering abstraction in `catalog/`,
//! per-offering adapters in `offerings/`, I/O in `infra/`, HTTP in `api/`.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

const OFFERING_NAME: &str = "zen-garden.ai.orchestrator";

#[derive(Parser)]
#[command(name = "zen-garden-ai-orchestrator")]
#[command(about = "AI Orchestrator — multi-offering AI service orchestration for Zen Garden")]
#[command(version)]
struct Cli {
    /// Koi endpoint for mDNS/DNS/UDP discovery capabilities.
    #[arg(long, env = "KOI_ENDPOINT", default_value = "http://host.docker.internal:5641")]
    koi_endpoint: String,

    /// Explicit stone endpoint (skips Koi discovery). Like Rake's `--at`.
    #[arg(long, env = "GARDEN_STONE")]
    stone: Option<String>,

    /// Dashboard port (management UI + API).
    #[arg(long, env = "AI_ORCH_DASHBOARD_PORT", default_value = "7190")]
    dashboard_port: u16,

    /// Data directory for config and metrics persistence.
    #[arg(long, env = "AI_ORCH_DATA_DIR", default_value = "/data")]
    data_dir: String,

    /// Log level.
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .init();

    tracing::info!(
        offering = OFFERING_NAME,
        koi = %cli.koi_endpoint,
        stone = ?cli.stone,
        dashboard_port = cli.dashboard_port,
        version = env!("CARGO_PKG_VERSION"),
        "starting AI orchestrator"
    );

    // Data directory
    tokio::fs::create_dir_all(&cli.data_dir).await?;

    // TODO: Block 2 — wire startup sequence, discovery, gateway, proxy ports
    tracing::info!("AI orchestrator skeleton running — Block 1 complete, awaiting Block 2 wiring");

    // Hold open until Ctrl+C
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");

    Ok(())
}
