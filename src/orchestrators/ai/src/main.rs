//! Zen Garden AI Orchestrator — bootstrap binary (ORCH-0030 R2 M3).
//!
//! Responsibilities of this file:
//! - Parse CLI arguments (just the orchestrator's own knobs).
//! - Initialize logging.
//! - Construct the data-directory-backed stores.
//! - Construct the unified `EventBus`, `Resources`,
//!   `CapabilityDirectory`, `DirectorySubscriber`, and
//!   `ProviderRegistry`.
//! - Resolve the tended stone (explicit `--stone` overrides Koi
//!   discovery).
//! - Construct every M1 adapter, register each into the
//!   `ProviderRegistry`. Each adapter publishes a
//!   `CapabilityAnnouncement` to the bus as it discovers instances.
//! - Load cloud-provider secrets from `{data_dir}/cloud_providers.json`
//!   and register cloud providers when keys are present (M1 has
//!   only Google/Gemini; Anthropic and OpenAI return in M2).
//! - Mount the HTTP router and serve until shutdown.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use zen_garden_ai_orchestrator::{
    app_state::AppState,
    domain::{
        events::EventBus,
        idempotency::IdempotencyStore,
        jobs::JobStore,
        media::MediaStore,
        resources::Resources,
        vocabulary::VocabularyRegistry,
    },
    http::router,
    providers::{
        comfyui::{ComfyUiConfig, ComfyUiProvider},
        docling::{DoclingConfig, DoclingProvider},
        google::{GoogleConfig, GoogleProvider},
        kokoro::{KokoroConfig, KokoroProvider},
        libretranslate::{LibreTranslateConfig, LibreTranslateProvider},
        ollama::{OllamaConfig, OllamaProvider},
        openedai_speech::{OpenedaiSpeechConfig, OpenedaiSpeechProvider},
        speaches::{SpeachesConfig, SpeachesProvider},
        whispercpp::{WhisperCppConfig, WhisperCppProvider},
    },
    services::{
        catalog_builder::CatalogBuilder,
        cloud_secrets::CloudSecrets,
        contextualizer::Contextualizer,
        directory_subscriber::{CapabilityDirectory, DirectorySubscriber},
        dispatcher::Dispatcher,
        garden_discovery::GardenDiscovery,
        idempotency_store::InMemoryIdempotencyStore,
        job_store::DiskJobStore,
        media_resolver::MediaResolver,
        media_store::DiskMediaStore,
        provider_registry::ProviderRegistry,
    },
};

#[derive(Parser)]
#[command(name = "zen-garden-ai-orchestrator")]
#[command(about = "AI Orchestrator (ORCH-0030 R2)")]
#[command(version)]
struct Cli {
    /// Listen port for the `/v1/*` surface, `/health`, and `/metrics`.
    #[arg(long, env = "AI_ORCH_PORT", default_value = "7190")]
    port: u16,

    /// Data directory for media, jobs, and `cloud_providers.json`.
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
        "starting Zen Garden AI Orchestrator (ORCH-0030 R2)"
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
    let idempotency_store: Arc<dyn IdempotencyStore> =
        Arc::new(InMemoryIdempotencyStore::new());

    // ── Event bus (ORCH-0030 §1) ────────────────────────────────
    //
    // The unified nervous system. Every domain publishes state
    // transitions here; HTTP `/v1/events` exposes a glob-filtered
    // view. Built before any domain constructor so they can capture
    // it by handle at wiring time.
    let events = EventBus::new();

    // ── Resources domain (ORCH-0030 §2) ─────────────────────────
    //
    // Physical stone resource accounting. Topology is hydrated by
    // garden discovery as stones come online; adapters claim
    // resources before dispatching work and release on completion.
    let resources = Resources::new(events.clone());

    // ── Vocabulary ──────────────────────────────────────────────
    let vocabularies = VocabularyRegistry::build();

    // ── Provisioning queue (ORCH-0029 Phase 2) ──────────────────
    //
    // Bounded-concurrency worker that downloads missing models
    // into the local dependency cache and pushes them to
    // discovered ComfyUI instances. ComfyUI submits jobs at
    // discovery time.
    let provisioning =
        zen_garden_ai_orchestrator::services::skills::ProvisioningQueue::with_default_concurrency();

    // ── Capability directory + subscriber (ORCH-0030 §R2.2) ────
    //
    // The CapabilityDirectory is the authoritative routing view.
    // The DirectorySubscriber consumes
    // `directory.provider.{name}.capabilities` events from the bus
    // and rebuilds the directory wholesale on each accepted
    // announcement.
    let capability_directory = CapabilityDirectory::new();
    let directory_subscriber =
        DirectorySubscriber::new(capability_directory.clone(), events.clone());

    // ── Provider registry (ORCH-0030 R2 M3) ─────────────────────
    //
    // Process-internal `name → Arc<dyn Provider>` lookup. The
    // dispatcher reads from this to invoke `provider.onboard()`
    // on the provider chosen by the contextualizer.
    let provider_registry = ProviderRegistry::new();

    let shutdown = CancellationToken::new();

    // ── Spawn directory_subscriber EARLY ────────────────────────
    //
    // The DirectorySubscriber consumes capability announcements
    // from the EventBus. The bus is a broadcast channel: late
    // subscribers MISS events that were published before they
    // subscribed. If we waited until after adapter construction,
    // any adapter that publishes during its `new()` (e.g. the
    // cloud Google adapter, or any future adapter that fires an
    // initial announcement during startup) would publish into a
    // void and never appear in `CapabilityDirectory`.
    //
    // Spawning here, before adapters are built, guarantees the
    // subscriber is consuming the bus from tick zero.
    let directory_subscriber_handle =
        tokio::spawn(directory_subscriber.clone().run(shutdown.clone()));

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
    // Each adapter takes the discovery handle and the event bus,
    // then immediately spawns its own subscriber task for the FQNs
    // it claims. Adapters publish `CapabilityAnnouncement` events
    // to the bus on every state change; the DirectorySubscriber
    // builds the routing view from those events.
    //
    // ComfyUI also takes the provisioning queue (it submits jobs
    // when discovery surfaces an instance missing required models).
    let ollama = OllamaProvider::new(
        OllamaConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let libretranslate = LibreTranslateProvider::new(
        LibreTranslateConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let kokoro = KokoroProvider::new(
        KokoroConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let openedai_speech = OpenedaiSpeechProvider::new(
        OpenedaiSpeechConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let whispercpp = WhisperCppProvider::new(
        WhisperCppConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let speaches = SpeachesProvider::new(
        SpeachesConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let docling = DoclingProvider::new(
        DoclingConfig::default(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    );
    let comfyui = ComfyUiProvider::new(
        ComfyUiConfig {
            skills_dir: data_dir.join("skills").join("comfyui"),
            data_dir: data_dir.clone(),
        },
        provisioning.clone(),
        discovery.clone(),
        events.clone(),
        shutdown.clone(),
    )
    .await;

    // Register every local adapter into the provider registry.
    provider_registry.register(ollama.clone()).await;
    provider_registry.register(libretranslate.clone()).await;
    provider_registry.register(kokoro.clone()).await;
    provider_registry.register(openedai_speech.clone()).await;
    provider_registry.register(whispercpp.clone()).await;
    provider_registry.register(speaches.clone()).await;
    provider_registry.register(docling.clone()).await;
    provider_registry.register(comfyui.clone()).await;
    tracing::info!("8 local adapters registered");

    // ── Cloud providers (loaded from {data_dir}/cloud_providers.json) ──
    //
    // M1 ships only Google/Gemini. Anthropic and OpenAI return in
    // M2 — see `MILESTONE-1-PLAN.md` §M7.
    let cloud = CloudSecrets::load(&data_dir).await;
    if let Some(s) = cloud.google {
        let google = GoogleProvider::new(
            GoogleConfig {
                base_url: s.base_url,
                api_key: s.api_key,
            },
            events.clone(),
        );
        provider_registry.register(google).await;
        tracing::info!("registered Google provider from cloud_providers.json");
    }

    // ── Pipeline services ───────────────────────────────────────
    let contextualizer = Arc::new(Contextualizer::new(vocabularies.clone()));
    let media_resolver = Arc::new(MediaResolver);
    let dispatcher = Arc::new(Dispatcher::new(
        capability_directory.clone(),
        provider_registry.clone(),
        contextualizer.clone(),
        media_resolver.clone(),
        idempotency_store.clone(),
        job_store.clone(),
        media_store.clone(),
    ));
    let catalog = CatalogBuilder::new(
        capability_directory.clone(),
        vocabularies.clone(),
        events.clone(),
    );

    // ── Preferences (ORCH-0030 §8) ──────────────────────────────
    let preferences = crate::domain::preferences::Preferences::load(
        &data_dir,
        events.clone(),
    )
    .await;

    // ── Shared AppState ─────────────────────────────────────────
    let state = AppState {
        vocabularies: vocabularies.clone(),
        media_store: media_store.clone(),
        job_store: job_store.clone(),
        idempotency_store: idempotency_store.clone(),
        dispatcher,
        catalog: catalog.clone(),
        provisioning: provisioning.clone(),
        data_dir: data_dir.clone(),
        events: events.clone(),
        resources: resources.clone(),
        capability_directory: capability_directory.clone(),
        provider_registry: provider_registry.clone(),
        preferences,
    };

    // ── Background tasks ────────────────────────────────────────
    let catalog_handle = tokio::spawn(catalog.clone().run(shutdown.clone()));

    // The directory_subscriber task was spawned earlier (before
    // adapter construction) so the EventBus consumer was running
    // from tick zero. The handle is held on the same `let
    // directory_subscriber_handle = ...` binding declared above
    // and joined alongside the other background tasks at shutdown.

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
            directory_subscriber_handle,
            catalog_handle,
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
