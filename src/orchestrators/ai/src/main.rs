//! AI Orchestrator entry point.
//!
//! Initializes the offering catalog, spawns background tasks, and binds the
//! proxy and dashboard HTTP servers.

use std::sync::Arc;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{fmt, EnvFilter};

use zen_garden_ai_orchestrator::api;
use zen_garden_ai_orchestrator::app_state::AppState;
use zen_garden_ai_orchestrator::catalog::OfferingRegistry;
use zen_garden_ai_orchestrator::domain::types::RouterConfig;
use zen_garden_ai_orchestrator::offerings::ollama::OllamaOffering;
use zen_garden_ai_orchestrator::tasks;

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
    #[arg(long, default_value = ".", env = "ZG_DATA_DIR")]
    data_dir: String,
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

    // ── Offering Catalog ────────────────────────────────────────
    let mut catalog = OfferingRegistry::new();
    catalog.register(Arc::new(OllamaOffering::new()));
    // Future: catalog.register(Arc::new(ComfyUiOffering::new()));
    // Future: catalog.register(Arc::new(SpeachesOffering::new()));

    tracing::info!(
        offerings = catalog.len(),
        "offering catalog initialized"
    );

    // ── Channels ────────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let (metrics_tx, metrics_rx) = mpsc::unbounded_channel();
    let (snapshot_tx, snapshot_rx) = watch::channel(serde_json::Value::Null);
    // Keep the sender alive until snapshot_publisher task is wired (Phase 2).
    // Dropping it would close the channel and break any Phase 2 consumer.
    let _snapshot_tx = snapshot_tx;

    // ── Config ──────────────────────────────────────────────────
    let config = load_config(&cli.data_dir).await;

    // ── App State ───────────────────────────────────────────────
    let state = AppState::new(
        catalog,
        cli.koi.clone(),
        cli.stone.clone(),
        cli.proxy_port,
        cli.dashboard_port,
        cli.data_dir.clone(),
        config,
        shutdown.clone(),
        snapshot_rx,
        metrics_tx,
    );

    // ── Restore persisted state ─────────────────────────────────
    state.load_tending().await;

    // ── Spawn background tasks ──────────────────────────────────
    let metrics_state = state.clone();
    let metrics_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::metrics_processor::run(metrics_state, metrics_rx, metrics_shutdown).await;
    });

    let health_state = state.clone();
    let health_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::health_check::run(health_state, health_shutdown).await;
    });

    let flush_state = state.clone();
    let flush_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::metrics_flush::run(flush_state, flush_shutdown).await;
    });

    // ── Proxy Server ────────────────────────────────────────────
    let proxy_app = Router::new()
        .fallback(api::proxy::proxy_handler)
        .with_state(state.clone())
        .layer(CorsLayer::permissive());

    let proxy_addr = format!("0.0.0.0:{}", cli.proxy_port);
    let proxy_listener = tokio::net::TcpListener::bind(&proxy_addr).await?;
    tracing::info!(addr = %proxy_addr, "proxy server listening");

    // ── Dashboard Server ────────────────────────────────────────
    let dashboard_app = Router::new()
        .route("/health", get(api::health::health))
        .route("/v1/models", get(api::extension::list_models))
        .route("/v1/stones", get(api::extension::list_stones))
        .route("/v1/capabilities", get(api::extension::list_capabilities))
        .with_state(state.clone())
        .layer(CorsLayer::permissive());

    let dashboard_addr = format!("0.0.0.0:{}", cli.dashboard_port);
    let dashboard_listener = tokio::net::TcpListener::bind(&dashboard_addr).await?;
    tracing::info!(addr = %dashboard_addr, "dashboard server listening");

    // ── Run servers ─────────────────────────────────────────────
    let proxy_shutdown = shutdown.clone();
    let dashboard_shutdown = shutdown.clone();

    tokio::select! {
        r = axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await }) => {
            if let Err(e) = r {
                tracing::error!(error = %e, "proxy server error");
            }
        }
        r = axum::serve(dashboard_listener, dashboard_app)
            .with_graceful_shutdown(async move { dashboard_shutdown.cancelled().await }) => {
            if let Err(e) = r {
                tracing::error!(error = %e, "dashboard server error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, initiating shutdown");
            shutdown.cancel();
        }
    }

    // Grace period for background tasks.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    tracing::info!("AI Orchestrator shut down");
    Ok(())
}

/// Load router config from data directory, or use defaults.
async fn load_config(data_dir: &str) -> RouterConfig {
    let path = std::path::Path::new(data_dir).join("router-config.toml");
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(config) => {
                tracing::info!(path = %path.display(), "loaded config");
                config
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid config, using defaults");
                RouterConfig::default()
            }
        },
        Err(_) => {
            tracing::info!("no config file, using defaults");
            RouterConfig::default()
        }
    }
}
