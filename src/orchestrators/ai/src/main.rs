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

use zen_garden_ai_orchestrator::api;
use zen_garden_ai_orchestrator::catalog::ProviderRegistry;
use zen_garden_ai_orchestrator::domain::types::OfferingKind;
use zen_garden_ai_orchestrator::infra::persistence;
use zen_garden_ai_orchestrator::offerings::cloud::CloudProviderStore;
use zen_garden_ai_orchestrator::offerings::ollama::OllamaClient;
use zen_garden_ai_orchestrator::providers;
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

    // ── Provider Registry ───────────────────────────────────────────
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(providers::ollama::OllamaProvider::new()));
    providers.register(Arc::new(providers::openai::OpenAiProvider::new()));
    providers.register(Arc::new(providers::anthropic::AnthropicProvider::new()));
    providers.register(Arc::new(providers::google::GoogleProvider::new()));
    providers.register(Arc::new(providers::infinity::InfinityProvider::new()));
    providers.register(Arc::new(providers::openedai_speech::OpenedaiSpeechProvider::new()));
    providers.register(Arc::new(providers::libretranslate::LibreTranslateProvider::new()));
    providers.register(Arc::new(providers::speaches::SpeachesProvider::new()));
    providers.register(Arc::new(providers::whispercpp::WhisperCppProvider::new()));
    providers.register(Arc::new(providers::docling::DoclingProvider::new()));
    providers.register(Arc::new(providers::kokoro::KokoroProvider::new()));
    providers.register(Arc::new(providers::comfyui::ComfyUiProvider::new()));

    // ── Cloud Providers ─────────────────────────────────────────────
    let cloud_store = CloudProviderStore::load(&cli.data_dir).await;

    // ── Secrets ────────────────────────────────────────────────────
    let secrets = zen_garden_ai_orchestrator::infra::secrets::SecretsStore::load(
        std::path::Path::new(&cli.data_dir),
    ).await;

    let provider_count = providers.len();
    tracing::info!(providers = provider_count, "provider registry initialized");

    // ── Ollama Client ───────────────────────────────────────────────
    let ollama_client = OllamaClient::default();

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
        providers,
        ollama_client,
        cloud_store,
        secrets,
        shutdown.clone(),
        metrics_tx,
    );

    // ── Load Skills from Disk ───────────────────────────────────────
    // 1. Seed embedded skills to {data_dir}/skills/ (version-gated)
    // 2. Scan disk, parse skill.json, resolve workflows
    // 3. Register in SkillRegistry — disk is the sole source of truth
    {
        let skills_dir = std::path::PathBuf::from(&cli.data_dir).join("skills");
        zen_garden_ai_orchestrator::skills::loader::seed_embedded_skills(&skills_dir).await;
        let skills = zen_garden_ai_orchestrator::skills::loader::load_skills(&skills_dir).await;
        let count = skills.len();
        for skill in skills {
            state.skills.register(skill).await;
        }
        if count > 0 {
            tracing::info!(skills = count, "skills loaded from disk repository");
        }
    }

    // ── Restore Persisted State ─────────────────────────────────────
    state.load_tending().await;

    {
        let snapshot = persistence::load_metrics(&cli.data_dir).await;
        state.observability.restore_metrics(snapshot).await;
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

    let cloud_sync_handle = tokio::spawn(tasks::cloud_sync::run(
        state.clone(),
        shutdown.clone(),
    ));

    // Intelligence reactive loop (ORCH-0020): subscribes to Registry + Directory
    // changes, recomputes recommendations asynchronously.
    {
        let intel_runner = zen_garden_ai_orchestrator::domain::intelligence::IntelligenceRunner::new(
            state.intelligence.tx_clone(),
            state.registry.subscribe(),
            state.directory.subscribe(),
        );
        let intel_config = state.config.clone();
        let intel_bench = state.benchmark_run.clone();
        let intel_shutdown = shutdown.clone();
        tokio::spawn(async move {
            intel_runner.run(intel_config, intel_bench, intel_shutdown).await;
        });
    }

    tracing::info!(
        tasks = 7,
        "background tasks spawned (discovery, gateway, health, metrics_flush, metrics_processor, cloud_sync, intelligence)"
    );

    // ── CORS — wide open for all APIs and proxy ports ────────────
    let cors = tower_http::cors::CorsLayer::permissive();

    // ── Dashboard Server ────────────────────────────────────────────
    let dashboard_router = axum::Router::new()
        // API endpoints
        .route("/health", axum::routing::get(api::health::health))
        .route("/api/status", axum::routing::get(api::dashboard::get_status))
        .route("/api/events", axum::routing::get(api::dashboard::get_events))
        .route("/api/settings", axum::routing::get(api::dashboard::get_settings))
        .route("/api/settings", axum::routing::post(api::dashboard::post_settings))
        .route("/api/jobs", axum::routing::get(api::dashboard::get_jobs))
        .route("/api/directory", axum::routing::get(api::dashboard::get_directory))
        .route("/api/defaults", axum::routing::get(api::dashboard::get_defaults))
        .route("/api/defaults", axum::routing::post(api::dashboard::post_defaults))
        .route("/api/providers", axum::routing::get(api::dashboard::get_providers))
        .route("/api/providers", axum::routing::post(api::dashboard::add_provider))
        .route("/api/providers/{name}", axum::routing::delete(api::dashboard::delete_provider))
        .route("/api/providers/{name}/toggle", axum::routing::patch(api::dashboard::toggle_provider))
        .route("/api/providers/test", axum::routing::post(api::provider_test::test_key))
        // Service management actions
        .route("/api/services/{offering}/pull", axum::routing::post(api::service_actions::pull_model))
        .route("/api/services/{offering}/refresh", axum::routing::post(api::service_actions::refresh_models))
        .route("/api/services/{offering}/benchmark", axum::routing::post(api::service_actions::trigger_benchmark))
        .route("/api/services/{offering}/sync", axum::routing::post(api::service_actions::sync_models))
        .route("/api/services/{offering}/load", axum::routing::post(api::service_actions::load_model))
        .route("/api/services/{offering}/unload", axum::routing::post(api::service_actions::unload_model))
        .route("/api/services/{offering}/models/{model}", axum::routing::delete(api::service_actions::delete_model))
        // Unified inference API (ORCH-0016)
        .route("/v1/chat/completions", axum::routing::post(api::unified::chat_completions))
        .route("/v1/embeddings", axum::routing::post(api::unified::embeddings))
        .route("/v1/audio/speech", axum::routing::post(api::unified::speech))
        .route("/v1/audio/transcriptions", axum::routing::post(api::unified::transcriptions))
        .route("/v1/models", axum::routing::get(api::unified::models))
        .route("/v1/models/{model}/form", axum::routing::get(api::unified::model_form))
        // Secrets management
        .route("/v1/secrets", axum::routing::get(api::secrets::list_secrets))
        .route("/v1/secrets/{key}", axum::routing::post(api::secrets::set_secret))
        .route("/v1/secrets/{key}", axum::routing::delete(api::secrets::delete_secret))
        // Service skill management — provider-scoped (ORCH-0023)
        .route("/v1/services/{provider}/skills", axum::routing::get(api::skill_manage::list_skills))
        .route("/v1/services/{provider}/skills/new", axum::routing::get(api::skill_manage::new_skill))
        .route("/v1/services/{provider}/skills/analyze", axum::routing::get(api::skill_manage::analyze_skill))
        .route("/v1/services/{provider}/skills/{moniker}", axum::routing::get(api::skill_manage::get_skill))
        .route("/v1/services/{provider}/skills/{moniker}", axum::routing::post(api::skill_manage::upsert_skill))
        .route("/v1/services/{provider}/skills/{moniker}", axum::routing::delete(api::skill_manage::delete_skill))
        .route("/v1/services/{provider}/skills/{moniker}/workflows", axum::routing::get(api::skill_manage::list_workflows))
        .route("/v1/services/{provider}/skills/{moniker}/workflows/{name}", axum::routing::get(api::skill_manage::get_workflow))
        .route("/v1/services/{provider}/skills/{moniker}/workflows/{name}", axum::routing::put(api::skill_manage::put_workflow))
        // Skill endpoints — capability-namespaced (ORCH-0018)
        .route("/v1/{capability}/skill/{moniker}", axum::routing::post(api::workflows::invoke_skill))
        .route("/v1/jobs/{id}", axum::routing::get(api::workflows::get_job))
        .route("/v1/jobs/{id}/assets/{filename}", axum::routing::get(api::workflows::get_job_asset))
        .route("/v1/skills", axum::routing::get(api::workflows::list_skills))
        .route("/v1/skills/{skill}/form", axum::routing::get(api::workflows::skill_form))
        .with_state(state.clone())
        // Embedded dashboard SPA + static assets
        .route("/", axum::routing::get(api::static_files::index))
        .fallback(api::static_files::fallback)
        .layer(cors.clone())
        // No body size limit — images can be multi-megabyte
        .layer(axum::extract::DefaultBodyLimit::disable());

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

    // ── Ollama Proxy Server (port 21434) ────────────────────────────
    let proxy_port = OfferingKind::Ollama
        .proxy_port()
        .expect("Ollama has a proxy port");

    let proxy_state = api::proxy::ProxyState {
        app: state.clone(),
        client: state.ollama_client.clone(),
    };

    let proxy_router = axum::Router::new()
        .fallback(api::proxy::proxy_handler)
        .with_state(proxy_state)
        .layer(cors.clone());

    let proxy_addr = std::net::SocketAddr::from(([0, 0, 0, 0], proxy_port));
    let proxy_listener = tokio::net::TcpListener::bind(proxy_addr).await?;
    tracing::info!(port = proxy_port, "Ollama proxy server listening");

    let proxy_shutdown = shutdown.clone();
    let proxy_handle = tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_router)
            .with_graceful_shutdown(proxy_shutdown.cancelled_owned())
            .await
            .ok();
    });

    // ── Generic Proxy Servers (non-Ollama offerings) ──────────────────
    // Each registered offering with a proxy port gets a pass-through proxy.
    let generic_proxy_kinds = [
        OfferingKind::Infinity,
        OfferingKind::OpenedaiSpeech,
        OfferingKind::LibreTranslate,
        OfferingKind::Speaches,
        OfferingKind::ComfyUi,
    ];

    let mut generic_proxy_handles = Vec::new();
    for kind in generic_proxy_kinds {
        // Only start if the provider is registered AND the offering has a proxy port
        if state.providers.get(kind).is_none() {
            continue;
        }
        let port = match kind.proxy_port() {
            Some(p) => p,
            None => continue,
        };

        let gp_state = api::generic_proxy::GenericProxyState {
            app: state.clone(),
            kind,
        };

        let router = axum::Router::new()
            .fallback(api::generic_proxy::proxy_handler)
            .with_state(gp_state)
            .layer(cors.clone());

        match tokio::net::TcpListener::bind(std::net::SocketAddr::from(([0, 0, 0, 0], port))).await
        {
            Ok(listener) => {
                tracing::info!(port = port, kind = %kind, "generic proxy server listening");
                let shutdown = shutdown.clone();
                generic_proxy_handles.push(tokio::spawn(async move {
                    axum::serve(listener, router)
                        .with_graceful_shutdown(shutdown.cancelled_owned())
                        .await
                        .ok();
                }));
            }
            Err(e) => {
                tracing::warn!(port = port, kind = %kind, error = %e, "failed to bind generic proxy port");
            }
        }
    }

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
            cloud_sync_handle,
            health_handle,
            metrics_flush_handle,
            metrics_proc_handle,
            dashboard_handle,
            proxy_handle,
        );
        for h in generic_proxy_handles {
            let _ = h.await;
        }
    })
    .await;

    tracing::info!("AI orchestrator stopped");
    Ok(())
}
