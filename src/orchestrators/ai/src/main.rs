//! Zen Garden AI Orchestrator — bootstrap binary (ORCH-0028).
//!
//! Responsibilities of this file:
//! - Parse CLI arguments (just the orchestrator's own knobs — no
//!   per-provider configuration).
//! - Initialize logging.
//! - Construct the data-directory-backed stores.
//! - Construct the Directory aggregate, vocabularies, dispatcher.
//! - Resolve the tended stone (explicit `--stone` overrides Koi
//!   discovery).
//! - Construct every local provider with an empty instance pool.
//! - Spawn the garden discovery task that populates each provider's
//!   pool from the tended stone's topology endpoint.
//! - Load cloud-provider secrets from `{data_dir}/cloud_providers.json`
//!   and register cloud providers when keys are present.
//! - Mount the HTTP router and serve until shutdown.
//!
//! Everything else lives in [`domain`], [`services`], [`providers`],
//! and [`http`].

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use zen_garden_ai_orchestrator::{
    app_state::AppState,
    domain::{
        directory::Directory,
        idempotency::IdempotencyStore,
        jobs::JobStore,
        media::MediaStore,
        vocabulary::VocabularyRegistry,
    },
    http::router,
    providers::{
        anthropic::{AnthropicConfig, AnthropicProvider},
        comfyui::{ComfyUiConfig, ComfyUiProvider},
        docling::{DoclingConfig, DoclingProvider},
        google::{GoogleConfig, GoogleProvider},
        infinity::{InfinityConfig, InfinityProvider},
        kokoro::{KokoroConfig, KokoroProvider},
        libretranslate::{LibreTranslateConfig, LibreTranslateProvider},
        ollama::{OllamaConfig, OllamaProvider},
        openai::{OpenAiConfig, OpenAiProvider},
        openedai_speech::{OpenedaiSpeechConfig, OpenedaiSpeechProvider},
        speaches::{SpeachesConfig, SpeachesProvider},
        whispercpp::{WhisperCppConfig, WhisperCppProvider},
    },
    services::{
        catalog_builder::CatalogBuilder,
        cloud_secrets::CloudSecrets,
        contextualizer::Contextualizer,
        directory_maintenance,
        dispatcher::Dispatcher,
        garden_discovery::GardenDiscovery,
        idempotency_store::InMemoryIdempotencyStore,
        job_store::DiskJobStore,
        media_resolver::MediaResolver,
        media_store::DiskMediaStore,
        recommendation::{DemandLedger, PinRegistry, RecommendationEngine},
    },
};

#[derive(Parser)]
#[command(name = "zen-garden-ai-orchestrator")]
#[command(about = "AI Orchestrator (ORCH-0028)")]
#[command(version)]
struct Cli {
    /// Listen port for the `/v1/*` surface, `/health`, and `/metrics`.
    #[arg(long, env = "AI_ORCH_PORT", default_value = "7190")]
    port: u16,

    /// Data directory for media, jobs, recommendation pins, and
    /// `cloud_providers.json`.
    #[arg(long, env = "AI_ORCH_DATA_DIR", default_value = "/data")]
    data_dir: String,

    /// Log level.
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_level: String,

    /// Koi endpoint for mDNS-based stone discovery.
    #[arg(
        long,
        env = "KOI_ENDPOINT",
        default_value = "http://host.docker.internal:5641"
    )]
    koi_endpoint: String,

    /// Explicit stone HTTP endpoint (overrides Koi discovery).
    /// Example: `http://stone-quartz-fen:7185`.
    #[arg(long, env = "ZG_STONE")]
    stone: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .init();

    tracing::info!(
        port = cli.port,
        data_dir = %cli.data_dir,
        koi = %cli.koi_endpoint,
        stone_override = ?cli.stone,
        "starting Zen Garden AI Orchestrator (ORCH-0028)"
    );

    let data_dir = std::path::PathBuf::from(&cli.data_dir);
    tokio::fs::create_dir_all(&data_dir).await?;

    // ── Stores ──────────────────────────────────────────────────
    let media_store = DiskMediaStore::load(&data_dir)
        .await
        .map_err(|e| anyhow::anyhow!("media store: {e}"))?;
    let job_store = DiskJobStore::load(&data_dir)
        .await
        .map_err(|e| anyhow::anyhow!("job store: {e}"))?;
    let idempotency_store = Arc::new(InMemoryIdempotencyStore::new());

    // ── Vocabulary & Directory ──────────────────────────────────
    let vocabularies = VocabularyRegistry::build();
    let directory = Directory::new();

    // ── Skills aggregate (ORCH-0029) ────────────────────────────
    //
    // Parallel to the Directory: holds dynamic per-skill state
    // (registration metadata, per-instance readiness, AI naming
    // updates) that skill-aware adapters push at load and
    // provisioning-progress time.
    let skills = zen_garden_ai_orchestrator::services::skills::Skills::new();

    // ── Provisioning queue (ORCH-0029 Phase 2) ──────────────────
    //
    // Bounded-concurrency worker that downloads missing models
    // into the local dependency cache and pushes them to
    // discovered ComfyUI instances. Skill-aware adapters submit
    // jobs at discovery time.
    let provisioning = zen_garden_ai_orchestrator::services::skills::ProvisioningQueue::with_default_concurrency();

    // ── Recommendation engine ───────────────────────────────────
    let pins = Arc::new(PinRegistry::load(&data_dir).await);
    let demand = Arc::new(DemandLedger::new());
    let recommendation =
        RecommendationEngine::new(directory.clone(), pins.clone(), demand.clone());

    // ── Pipeline services ───────────────────────────────────────
    let resolver_adapter: Arc<
        dyn zen_garden_ai_orchestrator::services::contextualizer::RecommendationResolver,
    > = recommendation.clone();
    let contextualizer = Arc::new(Contextualizer::new(
        vocabularies.clone(),
        Some(resolver_adapter),
    ));
    let media_resolver = Arc::new(MediaResolver);
    let dispatcher = Arc::new(Dispatcher::new(
        directory.clone(),
        contextualizer.clone(),
        media_resolver.clone(),
        idempotency_store.clone(),
        demand.clone(),
        job_store.clone(),
        media_store.clone(),
    ));
    let catalog = CatalogBuilder::new(directory.clone(), vocabularies.clone(), skills.clone());

    let shutdown = CancellationToken::new();

    // ── Resolve tended stone ────────────────────────────────────
    //
    // Done before constructing the discovery service so the SSE
    // consumer has somewhere to connect from tick zero.
    let tended_stone = resolve_tended_stone(&cli)
        .await
        .context("resolve tended stone")?;
    tracing::info!(stone = %tended_stone, "tending stone");

    // ── Garden discovery (event-driven pub/sub) ────────────────
    //
    // Spawn the SSE consumer that watches the garden's tools
    // stream. Adapters subscribe by FQN at construction; the
    // discovery service emits events when matching offerings come
    // up or down.
    let discovery = GardenDiscovery::spawn(tended_stone, shutdown.clone());

    // ── Local providers ─────────────────────────────────────────
    //
    // Each provider takes the discovery handle and immediately
    // spawns its own subscriber task for the FQNs it claims. There
    // is no static offering map and no per-provider env var —
    // adapters self-declare which garden offerings they manage,
    // and discovery emits events as the topology changes.
    let ollama = OllamaProvider::new(
        OllamaConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let libretranslate = LibreTranslateProvider::new(
        LibreTranslateConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let infinity = InfinityProvider::new(
        InfinityConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let kokoro = KokoroProvider::new(
        KokoroConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let openedai_speech = OpenedaiSpeechProvider::new(
        OpenedaiSpeechConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let whispercpp = WhisperCppProvider::new(
        WhisperCppConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let speaches = SpeachesProvider::new(
        SpeachesConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let docling = DoclingProvider::new(
        DoclingConfig::default(),
        discovery.clone(),
        shutdown.clone(),
    );
    let comfyui = ComfyUiProvider::new(
        ComfyUiConfig {
            skills_dir: data_dir.join("skills").join("comfyui"),
            data_dir: data_dir.clone(),
        },
        skills.clone(),
        provisioning.clone(),
        discovery.clone(),
        shutdown.clone(),
    )
    .await;

    // Register them in the directory immediately. Each starts in
    // ProviderHealth::Offline; once discovery delivers events, the
    // provider publishes Healthy and the directory rebuilds.
    directory.register(ollama.clone()).await.ok();
    directory.register(libretranslate.clone()).await.ok();
    directory.register(infinity.clone()).await.ok();
    directory.register(kokoro.clone()).await.ok();
    directory.register(openedai_speech.clone()).await.ok();
    directory.register(whispercpp.clone()).await.ok();
    directory.register(speaches.clone()).await.ok();
    directory.register(docling.clone()).await.ok();
    directory.register(comfyui.clone()).await.ok();
    tracing::info!("9 local providers registered (instance pools start empty)");

    // ── Cloud providers (loaded from {data_dir}/cloud_providers.json) ──
    let cloud = CloudSecrets::load(&data_dir).await;
    if let Some(s) = cloud.anthropic {
        let provider = AnthropicProvider::new(AnthropicConfig {
            base_url: s.base_url,
            api_key: s.api_key,
        });
        directory.register(provider).await.ok();
        tracing::info!("registered Anthropic provider from cloud_providers.json");
    }
    if let Some(s) = cloud.openai {
        let provider = OpenAiProvider::new(OpenAiConfig {
            base_url: s.base_url,
            api_key: s.api_key,
            organization: s.organization,
        });
        directory.register(provider).await.ok();
        tracing::info!("registered OpenAI provider from cloud_providers.json");
    }
    if let Some(s) = cloud.google {
        let provider = GoogleProvider::new(GoogleConfig {
            base_url: s.base_url,
            api_key: s.api_key,
        });
        directory.register(provider).await.ok();
        tracing::info!("registered Google provider from cloud_providers.json");
    }

    // ── Shared AppState ─────────────────────────────────────────
    let state = AppState {
        directory: directory.clone(),
        vocabularies: vocabularies.clone(),
        media_store: media_store.clone(),
        job_store: job_store.clone(),
        idempotency_store: idempotency_store.clone(),
        dispatcher,
        recommendation: recommendation.clone(),
        catalog: catalog.clone(),
        skills: skills.clone(),
        provisioning: provisioning.clone(),
        data_dir: data_dir.clone(),
    };

    // ── Background tasks ────────────────────────────────────────
    let directory_maintenance_handle = tokio::spawn(directory_maintenance::run(
        directory.clone(),
        shutdown.clone(),
    ));
    let catalog_handle = tokio::spawn(catalog.clone().run(shutdown.clone()));
    let recommendation_handle = tokio::spawn(recommendation.clone().run(shutdown.clone()));

    // Garden discovery is already running — `GardenDiscovery::spawn`
    // launched its own SSE consumer above. Adapters subscribed at
    // construction time. Nothing to start here.
    let _ = &discovery;

    // Terminal reaper: release media reservations on job terminal.
    let reaper_jobs = job_store.clone();
    let reaper_media = media_store.clone();
    let reaper_shutdown = shutdown.clone();
    let terminal_reaper_handle = tokio::spawn(async move {
        let mut rx = reaper_jobs.subscribe_terminal();
        loop {
            tokio::select! {
                _ = reaper_shutdown.cancelled() => break,
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            if let Err(e) = reaper_media
                                .release_reservations_for_job(&event.id)
                                .await
                            {
                                tracing::warn!(
                                    job_id = %event.id,
                                    error = %e,
                                    "failed to release media reservations on job terminal"
                                );
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "terminal reaper lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // GC sweepers.
    let job_sweep_store = job_store.clone();
    let job_sweep_shutdown = shutdown.clone();
    let job_sweep_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(300));
        loop {
            tokio::select! {
                _ = job_sweep_shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let _ = job_sweep_store.sweep(chrono::Utc::now()).await;
                }
            }
        }
    });
    let idem_sweep_store = idempotency_store.clone();
    let idem_sweep_shutdown = shutdown.clone();
    let idem_sweep_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = idem_sweep_shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let _ = idem_sweep_store.sweep(chrono::Utc::now()).await;
                }
            }
        }
    });
    let media_sweep_store = media_store.clone();
    let media_sweep_shutdown = shutdown.clone();
    let media_sweep_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(600));
        loop {
            tokio::select! {
                _ = media_sweep_shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let filter = zen_garden_ai_orchestrator::domain::media::MediaFilter {
                        only_expired: true,
                        ..Default::default()
                    };
                    let _ = media_sweep_store.flush(filter).await;
                }
            }
        }
    });

    // ── HTTP server ─────────────────────────────────────────────
    let app = router::build(state).layer(tower_http::cors::CorsLayer::permissive());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], cli.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    let server_shutdown = shutdown.clone();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(server_shutdown.cancelled_owned())
            .await
            .ok();
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutdown signal received");
    shutdown.cancel();

    let timeout = Duration::from_secs(5);
    let _ = tokio::time::timeout(timeout, async {
        let _ = tokio::join!(
            directory_maintenance_handle,
            catalog_handle,
            recommendation_handle,
            terminal_reaper_handle,
            job_sweep_handle,
            idem_sweep_handle,
            media_sweep_handle,
            server_handle,
        );
    })
    .await;

    tracing::info!("orchestrator stopped");
    Ok(())
}

/// Resolve which stone to tend.
///
/// Precedence: explicit `--stone` > Koi mDNS discovery (first stone
/// returned). Fails if neither path yields a usable endpoint.
async fn resolve_tended_stone(cli: &Cli) -> Result<String> {
    if let Some(explicit) = cli.stone.as_ref() {
        return Ok(explicit.clone());
    }
    let stones = orchestrator_common::discovery::discover_stones(&cli.koi_endpoint)
        .await
        .context("discover stones via Koi")?;
    let chosen = stones.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!(
            "no stones discovered via Koi at {}; pass --stone to override",
            cli.koi_endpoint
        )
    })?;
    Ok(chosen.endpoint())
}
