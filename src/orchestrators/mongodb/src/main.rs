//! MongoDB Orchestrator (zen-garden.mongodb.orchestrator)
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.
//! All business logic lives in `domain/`, I/O in `infra/`, HTTP in `api/`.

use anyhow::Result;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use zen_garden_mongodb_orchestrator::api::{cluster, dashboard, health, monitoring};
use zen_garden_mongodb_orchestrator::tasks;
use zen_garden_mongodb_orchestrator::AppState;

#[derive(Parser)]
#[command(name = "zen-garden-mongodb-orchestrator")]
#[command(about = "MongoDB Orchestrator — replica set orchestration for Zen Garden")]
#[command(version)]
struct Cli {
    /// Koi endpoint for mDNS/DNS/UDP discovery capabilities.
    #[arg(long, env = "KOI_ENDPOINT", default_value = "http://localhost:5641")]
    koi_endpoint: String,

    /// Explicit stone endpoint (skips Koi discovery). Like Rake's `--at`.
    #[arg(long, env = "GARDEN_STONE")]
    stone: Option<String>,

    /// Offering name (for identification in the garden).
    #[arg(
        long,
        env = "GARDEN_OFFERING_NAME",
        default_value = "zen-garden.mongodb.orchestrator"
    )]
    offering_name: String,

    /// Dashboard port (management UI + API).
    #[arg(long, env = "MONGODB_ORCH_PORT", default_value = "7191")]
    dashboard_port: u16,

    /// Data directory for config and state persistence.
    #[arg(long, env = "MONGODB_ORCH_DATA_DIR", default_value = "/data")]
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
        dashboard_port = cli.dashboard_port,
        version = env!("CARGO_PKG_VERSION"),
        "MongoDB Orchestrator starting"
    );

    // ── Persistence ──────────────────────────────────────────────
    tokio::fs::create_dir_all(&cli.data_dir).await.ok();

    // ── Shared State ─────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let state = AppState::new(
        cli.offering_name.clone(),
        cli.koi_endpoint.clone(),
        cli.stone.clone(),
        cli.dashboard_port,
        cli.data_dir.clone(),
        shutdown.clone(),
    );

    // Load any cached tending state from a previous run
    state.load_tending().await;
    state.load_pending_actions().await;

    // ── Background Tasks ─────────────────────────────────────────
    let discovery_handle = tokio::spawn(tasks::discovery::run(
        state.clone(),
        shutdown.clone(),
    ));

    let bootstrap_handle = tokio::spawn(tasks::bootstrap::run(
        state.clone(),
        shutdown.clone(),
    ));

    let health_handle = tokio::spawn(tasks::health_monitor::run(
        state.clone(),
        shutdown.clone(),
    ));

    // Dynamic per-FQN gateway registration.
    // Registers a separate Moss gateway entry for each FQN group
    // (e.g., "mongodb", "mongodb:dev", "mongodb:prd") so that
    // `find mongodb:dev` resolves to the correct connection string.
    let gateway_handle = tokio::spawn(tasks::gateway::run(
        state.clone(),
        cli.koi_endpoint.clone(),
        cli.offering_name.clone(),
        shutdown.clone(),
    ));

    // ── Dashboard Server (:7191) ─────────────────────────────────
    let dashboard_router = Router::new()
        // Dashboard HTML
        .route("/", axum::routing::get(dashboard::get_dashboard))
        // Health
        .route("/health", axum::routing::get(health::health_check))
        // Dashboard API
        .route("/api/status", axum::routing::get(dashboard::get_status))
        .route("/api/events", axum::routing::get(dashboard::get_events))
        // Cluster management
        .route(
            "/api/cluster/status",
            axum::routing::get(cluster::get_cluster_status),
        )
        .route(
            "/api/cluster/members",
            axum::routing::get(cluster::get_cluster_members),
        )
        .route(
            "/api/cluster/connect",
            axum::routing::get(cluster::get_connection_strings),
        )
        .route(
            "/api/cluster/stepdown",
            axum::routing::post(cluster::post_stepdown),
        )
        .route(
            "/api/cluster/install",
            axum::routing::post(cluster::post_install),
        )
        .route(
            "/api/cluster/members/{endpoint}",
            axum::routing::delete(cluster::delete_member),
        )
        .route(
            "/api/cluster/actions",
            axum::routing::get(cluster::get_pending_actions),
        )
        // Monitoring
        .route(
            "/api/monitoring/oplog",
            axum::routing::get(monitoring::get_oplog),
        )
        .route(
            "/api/monitoring/cache",
            axum::routing::get(monitoring::get_cache),
        )
        .route(
            "/api/monitoring/lag",
            axum::routing::get(monitoring::get_lag),
        )
        .route(
            "/api/monitoring/placement",
            axum::routing::get(monitoring::get_placement),
        )
        .with_state(state.clone());

    let dashboard_addr = SocketAddr::from(([0, 0, 0, 0], cli.dashboard_port));
    let dashboard_listener = tokio::net::TcpListener::bind(dashboard_addr).await?;
    tracing::info!(addr = %dashboard_addr, "dashboard server listening");

    // ── Serve ────────────────────────────────────────────────────
    let dashboard_server = axum::serve(dashboard_listener, dashboard_router);

    // ── Graceful Shutdown ────────────────────────────────────────
    tokio::select! {
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
        let _ = bootstrap_handle.await;
        let _ = health_handle.await;
        let _ = gateway_handle.await;
    })
    .await
    .ok();

    tracing::info!("MongoDB Orchestrator stopped");
    Ok(())
}
