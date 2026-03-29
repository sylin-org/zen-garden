//! AI Orchestrator (zen-garden.ai.orchestrator)
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.
//! All business logic lives in `domain/`, offering abstraction in `catalog/`,
//! per-offering adapters in `offerings/`, I/O in `infra/`, HTTP in `api/`.

use anyhow::Result;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use std::sync::Arc;

use zen_garden_ai_orchestrator::catalog::OfferingRegistry;
use zen_garden_ai_orchestrator::infra::persistence;
use zen_garden_ai_orchestrator::offerings::ollama::OllamaOffering;
use zen_garden_ai_orchestrator::tasks;
use zen_garden_ai_orchestrator::AppState;

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

    // ── Logging ─────────────────────────────────────────────────────
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

    // ── Data Directory ──────────────────────────────────────────────
    tokio::fs::create_dir_all(&cli.data_dir).await?;

    // ── Config ──────────────────────────────────────────────────────
    let config = persistence::load_config(&cli.data_dir).await;

    // ── Offering Registry ───────────────────────────────────────────
    let mut registry = OfferingRegistry::new();
    registry.register(Arc::new(OllamaOffering::new()))?;
    let registered_count = registry.len();
    tracing::info!(offerings = registered_count, "offering registry initialized");

    // ── Channels ────────────────────────────────────────────────────
    let (metrics_tx, metrics_rx) = tokio::sync::mpsc::unbounded_channel();

    // ── Shared State ────────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let state = AppState::new(
        cli.koi_endpoint.clone(),
        cli.stone.clone(),
        cli.dashboard_port,
        cli.data_dir.clone(),
        config,
        registry,
        shutdown.clone(),
        metrics_tx,
    );

    // ── Restore Persisted State ─────────────────────────────────────
    state.load_tending().await;

    {
        let snapshot = persistence::load_metrics(&cli.data_dir).await;
        let mut metrics = state.metrics.write().await;
        metrics.restore_from_snapshot(snapshot);
    }

    // ── Background Tasks ────────────────────────────────────────────
    let discovery_handle = tokio::spawn(tasks::discovery::run(
        state.clone(),
        shutdown.clone(),
    ));

    let gateway_handle = tokio::spawn(tasks::gateway_announce::run(
        state.clone(),
        shutdown.clone(),
    ));

    let health_handle = tokio::spawn(tasks::health_check::run(
        state.clone(),
        shutdown.clone(),
    ));

    let metrics_flush_handle = tokio::spawn(tasks::metrics_flush::run(
        state.clone(),
        shutdown.clone(),
    ));

    let metrics_proc_handle = tokio::spawn(tasks::metrics_processor::run(
        state.clone(),
        metrics_rx,
        shutdown.clone(),
    ));

    tracing::info!(
        tasks = 5,
        "background tasks spawned (discovery, gateway, health, metrics_flush, metrics_processor)"
    );

    // ── Dashboard Server ────────────────────────────────────────────
    // TODO: Block 5 — React dashboard. For now, a health endpoint.
    let health_state = state.clone();
    let dashboard_router = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(move || {
                let state = health_state.clone();
                async move {
                    let instances = state.instances.read().await;
                    let total = instances.len();
                    let healthy = instances.values().filter(|i| i.is_routable()).count();
                    axum::Json(serde_json::json!({
                        "status": if healthy > 0 { "healthy" } else if total > 0 { "degraded" } else { "no_instances" },
                        "offering": OFFERING_NAME,
                        "version": env!("CARGO_PKG_VERSION"),
                        "instances": { "total": total, "healthy": healthy },
                    }))
                }
            }),
        );

    let dashboard_addr = std::net::SocketAddr::from(([0, 0, 0, 0], cli.dashboard_port));
    let dashboard_listener = tokio::net::TcpListener::bind(dashboard_addr).await?;
    tracing::info!(port = cli.dashboard_port, "dashboard server listening");

    let dashboard_shutdown = shutdown.clone();
    let dashboard_handle = tokio::spawn(async move {
        axum::serve(dashboard_listener, dashboard_router)
            .with_graceful_shutdown(dashboard_shutdown.cancelled_owned())
            .await
            .ok();
    });

    // ── Wait for Shutdown ───────────────────────────────────────────
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutdown signal received");
    shutdown.cancel();

    // Wait for tasks with timeout
    let timeout = std::time::Duration::from_secs(5);
    let _ = tokio::time::timeout(timeout, async {
        let _ = tokio::join!(
            discovery_handle,
            gateway_handle,
            health_handle,
            metrics_flush_handle,
            metrics_proc_handle,
            dashboard_handle,
        );
    })
    .await;

    tracing::info!("AI orchestrator stopped");
    Ok(())
}
