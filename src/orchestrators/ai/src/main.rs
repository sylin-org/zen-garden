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

    // ── Connectivity Probe ─────────────────────────────────────────
    // Verify we can reach infrastructure before wiring anything.

    // 1. Koi health
    let koi_healthy = orchestrator_common::discovery::check_koi_health(&cli.koi_endpoint).await;
    if koi_healthy {
        tracing::info!(endpoint = %cli.koi_endpoint, "koi: healthy");
    } else {
        tracing::warn!(endpoint = %cli.koi_endpoint, "koi: unreachable");
    }

    // 2. Stone discovery via Koi mDNS
    match orchestrator_common::discovery::discover_stones(&cli.koi_endpoint).await {
        Ok(stones) => {
            tracing::info!(count = stones.len(), "discovered stones via Koi");
            for s in &stones {
                tracing::info!(
                    name = %s.stone_name,
                    ip = %s.ip,
                    port = s.api_port,
                    health = ?s.health,
                    "  stone"
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "stone discovery failed");
        }
    }

    // 3. Topology query (via local Moss or explicit stone)
    let moss_endpoint = cli
        .stone
        .clone()
        .unwrap_or_else(|| format!("http://localhost:{}", garden_common::constants::MOSS_HTTP));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    match client.get(format!("{moss_endpoint}/api/v1/garden/topology")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            if let Some(entries) = body.get("data").and_then(|d| d.as_array()) {
                tracing::info!(stones = entries.len(), "topology query successful");
                for entry in entries {
                    let name = entry.get("stone_name").and_then(|v| v.as_str()).unwrap_or("?");
                    let health = entry.get("health").and_then(|v| v.as_str()).unwrap_or("?");
                    let services = entry
                        .get("services")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|s| s.get("offering").and_then(|o| o.as_str()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    tracing::info!(
                        stone = %name,
                        health = %health,
                        offerings = %services,
                        "  topology entry"
                    );
                }
            }
        }
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), "topology query returned non-200");
        }
        Err(e) => {
            tracing::warn!(error = %e, endpoint = %moss_endpoint, "topology query failed");
        }
    }

    tracing::info!("connectivity probe complete — Block 2 wiring next");

    // Hold open until Ctrl+C
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");

    Ok(())
}
