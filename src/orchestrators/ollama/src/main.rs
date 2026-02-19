//! Ollama Orchestrator (zen-garden.ollama.orchestrator)
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.
//! All business logic lives in `domain/`, I/O in `infra/`, HTTP in `api/`.

use anyhow::Result;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use zen_garden_ollama_orchestrator::api::{benchmark_api, dashboard, extension, health, management, proxy};
use zen_garden_ollama_orchestrator::infra::{ollama_client::OllamaClient, persistence};
use zen_garden_ollama_orchestrator::tasks;
use zen_garden_ollama_orchestrator::AppState;

#[derive(Parser)]
#[command(name = "zen-garden-ollama-orchestrator")]
#[command(about = "Ollama Orchestrator — VRAM-aware multi-instance orchestration")]
#[command(version)]
struct Cli {
    /// Koi endpoint for mDNS/DNS/UDP discovery capabilities.
    #[arg(long, env = "KOI_ENDPOINT", default_value = "http://localhost:5641")]
    koi_endpoint: String,

    /// Explicit stone endpoint (skips Koi discovery). Like Rake's `--at`.
    #[arg(long, env = "GARDEN_STONE")]
    stone: Option<String>,

    /// Offering name (for identification in the garden).
    #[arg(long, env = "GARDEN_OFFERING_NAME", default_value = "zen-garden.ollama.orchestrator")]
    offering_name: String,

    /// Proxy port (Ollama-compatible endpoint).
    /// Default 21434 avoids collision with local Ollama on 11434.
    #[arg(long, env = "ROUTER_PROXY_PORT", default_value = "21434")]
    proxy_port: u16,

    /// Dashboard port (management UI + API).
    #[arg(long, env = "ROUTER_DASHBOARD_PORT", default_value = "7190")]
    dashboard_port: u16,

    /// Data directory for config and metrics persistence.
    #[arg(long, env = "ROUTER_DATA_DIR", default_value = "/data")]
    data_dir: String,

    /// Log level.
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // ── Logging ──────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .init();

    tracing::info!(
        offering = %cli.offering_name,
        koi = %cli.koi_endpoint,
        stone = ?cli.stone,
        proxy_port = cli.proxy_port,
        dashboard_port = cli.dashboard_port,
        version = env!("CARGO_PKG_VERSION"),
        "Ollama Orchestrator starting"
    );

    // ── Persistence ──────────────────────────────────────────────
    tokio::fs::create_dir_all(&cli.data_dir).await.ok();
    let config = persistence::load_config(&cli.data_dir).await;

    // ── Channels (Shared Snapshot Space) ────────────────────────
    let (snapshot_tx, snapshot_rx) =
        tokio::sync::watch::channel(serde_json::json!({}));
    let (metrics_tx, metrics_rx) =
        tokio::sync::mpsc::unbounded_channel();

    // ── Shared State ─────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let state = AppState::new(
        cli.offering_name.clone(),
        cli.koi_endpoint.clone(),
        cli.stone.clone(),
        cli.proxy_port,
        cli.data_dir.clone(),
        config,
        shutdown.clone(),
        snapshot_rx,
        metrics_tx,
    );
    // Load any cached tending state from a previous run
    state.load_tending().await;

    // Load persisted benchmark run from fitness.json
    let bench_run = zen_garden_ollama_orchestrator::tasks::benchmark::load(&cli.data_dir).await;
    if !bench_run.gpu_matrix.entries.is_empty() {
        tracing::info!(results = bench_run.gpu_matrix.entries.len(), "restored gpu matrix");
    }
    *state.benchmark_run.write().await = bench_run;

    // Restore persisted metrics from /metrics folder
    let persisted = persistence::load_metrics(&cli.data_dir).await;
    if persisted.requests_total > 0 {
        let mut metrics = state.metrics.write().await;
        metrics.restore_from_snapshot(persisted);
        tracing::info!(
            requests = metrics.requests_total,
            stones = metrics.per_stone.len(),
            "restored persisted metrics"
        );
    }

    let client = OllamaClient::new();

    // ── Background Tasks ─────────────────────────────────────────
    let discovery_handle = tokio::spawn(tasks::discovery::run(
        state.clone(),
        client.clone(),
        shutdown.clone(),
    ));

    let reconciliation_handle = tokio::spawn(tasks::reconciliation::run(
        state.clone(),
        client.clone(),
        shutdown.clone(),
    ));

    let health_handle = tokio::spawn(tasks::health_check::run(
        state.clone(),
        client.clone(),
        shutdown.clone(),
    ));

    let metrics_handle = tokio::spawn(tasks::metrics_flush::run(
        state.clone(),
        shutdown.clone(),
    ));

    let model_sync_handle = tokio::spawn(tasks::model_sync::run(
        state.clone(),
        client.clone(),
        shutdown.clone(),
    ));

    let snapshot_handle = tokio::spawn(tasks::snapshot_publisher::run(
        state.clone(),
        snapshot_tx,
        shutdown.clone(),
    ));

    let metrics_proc_handle = tokio::spawn(tasks::metrics_processor::run(
        state.clone(),
        metrics_rx,
        shutdown.clone(),
    ));

    let placement_handle = tokio::spawn(tasks::placement::run(
        state.clone(),
        client.clone(),
        shutdown.clone(),
    ));

    let gateway_handle = tokio::spawn(tasks::gateway_announce::run(
        state.clone(),
        shutdown.clone(),
    ));

    // ── Proxy Server (:11434) ────────────────────────────────────
    let proxy_state = proxy::ProxyState {
        app: state.clone(),
        client: client.clone(),
    };

    let proxy_router = Router::new()
        .route("/v1/models", axum::routing::get(extension::get_models))
        .route("/v1/stones", axum::routing::get(extension::get_stones))
        .fallback(proxy::proxy_handler)
        .with_state(proxy_state);

    let proxy_addr = SocketAddr::from(([0, 0, 0, 0], cli.proxy_port));
    let proxy_listener = tokio::net::TcpListener::bind(proxy_addr).await?;
    tracing::info!(addr = %proxy_addr, "proxy server listening");

    // ── Dashboard Server (:7190) ─────────────────────────────────
    let mgmt_state = management::ManagementState {
        app: state.clone(),
        client: client.clone(),
    };

    let dashboard_router = Router::new()
        // Dashboard
        .route("/", axum::routing::get(dashboard::get_dashboard))
        .route("/api/status", axum::routing::get(dashboard::get_status))
        .route("/api/events", axum::routing::get(dashboard::get_events))
        // Settings
        .route(
            "/api/settings",
            axum::routing::get(dashboard::get_settings)
                .post(dashboard::post_settings),
        )
        // Metrics
        .route(
            "/api/metrics/reset",
            axum::routing::post(dashboard::post_metrics_reset),
        )
        // Jobs
        .route("/api/jobs", axum::routing::get(dashboard::get_jobs))
        // Health
        .route("/health", axum::routing::get(health::health_check))
        .with_state(state.clone())
        // Model management (needs ManagementState with client)
        .route(
            "/api/management/pull",
            axum::routing::post(management::pull_model),
        )
        .route(
            "/api/management/delete",
            axum::routing::post(management::delete_model),
        )
        .route(
            "/api/management/feasibility",
            axum::routing::get(management::check_feasibility),
        )
        .with_state(mgmt_state)
        // Benchmark API (needs BenchmarkState with client)
        .route(
            "/api/benchmark/start",
            axum::routing::post(benchmark_api::start_benchmark),
        )
        .route(
            "/api/benchmark/cancel",
            axum::routing::post(benchmark_api::cancel_benchmark),
        )
        .route(
            "/api/benchmark/results",
            axum::routing::get(benchmark_api::get_results),
        )
        .route(
            "/api/benchmark/export",
            axum::routing::get(benchmark_api::export_fitness),
        )
        .with_state(benchmark_api::BenchmarkState {
            app: state.clone(),
            client: client.clone(),
        });

    let dashboard_addr = SocketAddr::from(([0, 0, 0, 0], cli.dashboard_port));
    let dashboard_listener = tokio::net::TcpListener::bind(dashboard_addr).await?;
    tracing::info!(addr = %dashboard_addr, "dashboard server listening");

    // ── Serve ────────────────────────────────────────────────────
    let proxy_server = axum::serve(proxy_listener, proxy_router);
    let dashboard_server = axum::serve(dashboard_listener, dashboard_router);

    // ── Graceful Shutdown ────────────────────────────────────────
    tokio::select! {
        r = proxy_server => {
            tracing::warn!("proxy server exited: {:?}", r);
        }
        r = dashboard_server => {
            tracing::warn!("dashboard server exited: {:?}", r);
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown signal received");
        }
    }

    shutdown.cancel();

    // Wait for tasks to complete (with timeout)
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let _ = discovery_handle.await;
        let _ = reconciliation_handle.await;
        let _ = health_handle.await;
        let _ = metrics_handle.await;
        let _ = model_sync_handle.await;
        let _ = snapshot_handle.await;
        let _ = metrics_proc_handle.await;
        let _ = placement_handle.await;
        let _ = gateway_handle.await;
    })
    .await
    .ok();

    tracing::info!("Ollama Orchestrator stopped");
    Ok(())
}
