//! Garden Router — AI Capability Router (Ollama)
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.
//! All business logic lives in `domain/`, I/O in `infra/`, HTTP in `api/`.

use anyhow::Result;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use garden_router::api::{dashboard, health, management, proxy};
use garden_router::infra::{ollama_client::OllamaClient, persistence};
use garden_router::tasks;
use garden_router::AppState;

#[derive(Parser)]
#[command(name = "garden-router")]
#[command(about = "AI Capability Router — VRAM-aware Ollama orchestrator")]
#[command(version)]
struct Cli {
    /// Stone endpoint for Tools API access.
    #[arg(long, env = "GARDEN_STONE_ENDPOINT")]
    stone_endpoint: String,

    /// Offering name (for identification in the garden).
    #[arg(long, env = "GARDEN_OFFERING_NAME", default_value = "ai-router")]
    offering_name: String,

    /// Proxy port (Ollama-compatible endpoint).
    #[arg(long, env = "ROUTER_PROXY_PORT", default_value = "11434")]
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
        stone = %cli.stone_endpoint,
        proxy_port = cli.proxy_port,
        dashboard_port = cli.dashboard_port,
        version = env!("CARGO_PKG_VERSION"),
        "AI Capability Router starting"
    );

    // ── Persistence ──────────────────────────────────────────────
    tokio::fs::create_dir_all(&cli.data_dir).await.ok();
    let config = persistence::load_config(&cli.data_dir).await;

    // ── Shared State ─────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let state = AppState::new(
        cli.offering_name.clone(),
        cli.stone_endpoint.clone(),
        cli.data_dir.clone(),
        config,
        shutdown.clone(),
    );
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

    // ── Proxy Server (:11434) ────────────────────────────────────
    let proxy_state = proxy::ProxyState {
        app: state.clone(),
        client: client.clone(),
    };

    let proxy_router = Router::new()
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
        .with_state(mgmt_state);

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
    })
    .await
    .ok();

    tracing::info!("AI Capability Router stopped");
    Ok(())
}
