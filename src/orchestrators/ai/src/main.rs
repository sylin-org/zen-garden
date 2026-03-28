//! AI Orchestrator entry point.
//!
//! Initializes the offering catalog, spawns background tasks, and binds the
//! proxy and dashboard HTTP servers.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "zen-garden-ai-orchestrator")]
#[command(about = "Unified AI capability router for Zen Garden")]
struct Cli {
    /// Proxy listen port (Ollama-compatible + extensions).
    #[arg(long, default_value = "21434", env = "PROXY_PORT")]
    proxy_port: u16,

    /// Dashboard listen port.
    #[arg(long, default_value = "7190", env = "DASHBOARD_PORT")]
    dashboard_port: u16,

    /// Explicit stone endpoint (skip mDNS discovery).
    #[arg(long, env = "ZG_STONE")]
    stone: Option<String>,

    /// Koi mDNS endpoint for discovery.
    #[arg(long, default_value = "http://127.0.0.1:5353", env = "KOI_ENDPOINT")]
    koi: String,

    /// Data directory for config, metrics, fitness data.
    #[arg(long, env = "ZG_DATA_DIR")]
    data_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    tracing::info!(
        proxy_port = cli.proxy_port,
        dashboard_port = cli.dashboard_port,
        "Starting AI Orchestrator"
    );

    // TODO: Phase 1 — Initialize offering catalog, app state, background tasks,
    //       and HTTP servers. Placeholder until domain + tasks + API layers are wired.

    tracing::info!("AI Orchestrator shutting down");
    Ok(())
}
