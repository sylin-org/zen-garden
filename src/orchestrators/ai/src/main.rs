//! AI Orchestrator entry point.
//!
//! Initializes the offering catalog, spawns background tasks, and binds the
//! proxy and dashboard HTTP servers.

use std::sync::Arc;

use anyhow::Result;
use axum::routing::{delete, get, post, put};
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
use zen_garden_ai_orchestrator::offerings::comfyui::ComfyUiOffering;
use zen_garden_ai_orchestrator::offerings::infinity::InfinityOffering;
use zen_garden_ai_orchestrator::offerings::libretranslate::LibreTranslateOffering;
use zen_garden_ai_orchestrator::offerings::ollama::OllamaOffering;
use zen_garden_ai_orchestrator::offerings::openedai_speech::OpenedaiSpeechOffering;
use zen_garden_ai_orchestrator::offerings::speaches::SpeachesOffering;
use zen_garden_ai_orchestrator::offerings::whispercpp::WhisperCppOffering;
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
    // Local/garden offerings (always registered).
    catalog.register(Arc::new(OllamaOffering::new()));
    catalog.register(Arc::new(ComfyUiOffering::new()));
    catalog.register(Arc::new(WhisperCppOffering::new()));
    catalog.register(Arc::new(SpeachesOffering::new()));
    catalog.register(Arc::new(OpenedaiSpeechOffering::new()));
    catalog.register(Arc::new(InfinityOffering::new()));
    catalog.register(Arc::new(LibreTranslateOffering::new()));

    // Cloud providers (registered only if API key env vars are set).
    for cloud in zen_garden_ai_orchestrator::offerings::cloud::openai_compat::register_cloud_providers() {
        catalog.register(cloud);
    }

    tracing::info!(
        offerings = catalog.len(),
        "offering catalog initialized"
    );

    // ── Channels ────────────────────────────────────────────────
    let shutdown = CancellationToken::new();
    let (metrics_tx, metrics_rx) = mpsc::unbounded_channel();
    let (snapshot_tx, snapshot_rx) = watch::channel(serde_json::Value::Null);

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

    // Discovery: find stones, query topology, subscribe to Tools API SSE.
    let discovery_state = state.clone();
    let discovery_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::discovery::run(discovery_state, discovery_shutdown).await;
    });

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

    // Reconciliation: periodic drift detection on all instances.
    let reconcile_state = state.clone();
    let reconcile_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::reconciliation::run(reconcile_state, reconcile_shutdown).await;
    });

    // Gateway announce: register with Koi mDNS + per-offering Moss gateways.
    let gateway_state = state.clone();
    let gateway_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::gateway_announce::run(gateway_state, gateway_shutdown).await;
    });

    // Snapshot publisher: build dashboard JSON every 3s.
    let snapshot_state = state.clone();
    let snapshot_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::snapshot_publisher::run(snapshot_state, snapshot_tx, snapshot_shutdown).await;
    });

    // Benchmark: fitness profiling (triggered via /api/benchmark/start).
    let bench_state = state.clone();
    let bench_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::benchmark::run(bench_state, bench_shutdown).await;
    });

    // Placement: demand-weighted model→stone assignment (60s interval).
    let placement_state = state.clone();
    let placement_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::placement::run(placement_state, placement_shutdown).await;
    });

    // Resource sync: model replication across tier peers (60s interval).
    let sync_state = state.clone();
    let sync_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::resource_sync::run(sync_state, sync_shutdown).await;
    });

    // Cloud sync: register cloud provider instances + periodic model refresh.
    let cloud_state = state.clone();
    let cloud_shutdown = shutdown.clone();
    tokio::spawn(async move {
        tasks::cloud_sync::run(cloud_state, cloud_shutdown).await;
    });

    // ── Proxy Server ────────────────────────────────────────────
    //
    // Ollama-compat routes (specific paths) + management routes,
    // then fallback to the generic capability-routing proxy.
    let proxy_app = Router::new()
        // Ollama backward compatibility
        .route("/", get(api::compat::ollama_root))
        .route("/api/tags", get(api::compat::ollama_tags))
        .route("/api/ps", get(api::compat::ollama_ps))
        .route("/api/version", get(api::compat::ollama_version))
        // Ollama model management
        .route("/api/show", post(api::management::ollama_show))
        .route("/api/pull", post(api::management::ollama_pull))
        .route("/api/delete", delete(api::management::ollama_delete))
        // Extension API
        .route("/v1/models", get(api::extension::list_models))
        .route("/v1/stones", get(api::extension::list_stones))
        .route("/v1/capabilities", get(api::extension::list_capabilities))
        .route("/v1/recommendations", get(api::recommendations::get_recommendation))
        .route(
            "/v1/recommendations/:capability/pin",
            put(api::recommendations::pin_recommendation)
                .delete(api::recommendations::unpin_recommendation),
        )
        // Generic proxy fallback for all other paths
        .fallback(api::proxy::proxy_handler)
        .with_state(state.clone())
        .layer(CorsLayer::permissive());

    let proxy_addr = format!("0.0.0.0:{}", cli.proxy_port);
    let proxy_listener = tokio::net::TcpListener::bind(&proxy_addr).await?;
    tracing::info!(addr = %proxy_addr, "proxy server listening");

    // ── Dashboard Server ────────────────────────────────────────
    let dashboard_app = Router::new()
        .route("/health", get(api::health::health))
        // Dashboard API
        .route("/api/status", get(api::dashboard::status))
        .route("/api/events", get(api::dashboard::events))
        .route(
            "/api/settings",
            get(api::dashboard::get_settings).post(api::dashboard::post_settings),
        )
        .route("/api/offerings", get(api::dashboard::offerings))
        .route("/api/jobs", get(api::dashboard::jobs))
        // Metrics
        .route("/api/metrics/reset", post(api::dashboard::reset_metrics))
        .route(
            "/api/metrics/model-counters/reset",
            post(api::dashboard::reset_model_counters),
        )
        // Model management (dashboard variants)
        .route("/api/management/pull", post(api::management::ollama_pull))
        .route("/api/management/delete", post(api::management::ollama_delete))
        .route("/api/management/feasibility", get(api::benchmark_api::management_feasibility))
        // Benchmark
        .route("/api/benchmark/start", post(api::benchmark_api::start_benchmark))
        .route("/api/benchmark/cancel", post(api::benchmark_api::cancel_benchmark))
        .route("/api/benchmark/results", get(api::benchmark_api::benchmark_results))
        .route("/api/benchmark/export", get(api::benchmark_api::benchmark_export))
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

    // Grace period for background tasks to complete shutdown handlers.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

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
