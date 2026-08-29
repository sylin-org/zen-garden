//! `moss` — the resident service of a Zen Garden stone. M0: presence only.
//!
//! Moss is the quiet green layer on every stone: it announces, listens,
//! and answers. Humans and agents walk the garden with `rake`.
//!
//! Startup is the typed pipeline (R0.4/L17): config -> ingress bind ->
//! dispatch -> topology claim -> expiry sweep → announcer → HTTP. Every
//! step's output feeds the next; failure aborts loudly by step name.
//! Shutdown speaks goodbye before the light goes out.

mod http;
mod identity;
mod jobs;
mod mcp;
mod pulse;
mod offerings;
mod source;

use clap::Parser;
use garden_kernel::announce::{self, ChirpSource};
use garden_kernel::config::{DiscoveryConfig, HttpConfig};
use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::Ingress;
use garden_kernel::pipeline;
use garden_kernel::topology::Topology;
use offerings::directory::OfferingsRoot;
use garden_kernel::responder;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Capacity of the queue between ingress and dispatch (datagrams).
const INGRESS_QUEUE: usize = 1024;
/// Grace period after cancellation for in-flight sends before goodbye.
const SHUTDOWN_GRACE_MS: u64 = 200;
#[derive(Parser)]
#[command(
    name = "moss",
    about = "A Zen Garden stone's resident service: announces itself, observes its peers.",
    version
)]
struct Cli {
    /// Stone name. Absent: a poetical name is minted on first boot and kept
    /// forever. Present: operator rename intent (the stone_id never changes).
    #[arg(long, env = "MOSS_STONE_NAME")]
    stone_name: Option<String>,

    /// HTTP port for this stone's surface.
    #[arg(long, env = "MOSS_HTTP_PORT", default_value_t = HttpConfig::DEFAULT_PORT)]
    http_port: u16,

    /// Discovery UDP port override (default is the v1 room).
    #[arg(long, env = "MOSS_DISCOVERY_PORT")]
    discovery_port: Option<u16>,

    /// Multicast group override (default is the v1 room).
    #[arg(long, env = "MOSS_MCAST_GROUP")]
    mcast_group: Option<Ipv4Addr>,
}

impl Cli {
    /// CLI > env > defaults (R3.7): v1 topology by default, then env twins,
    /// then CLI overrides. All deployment config lives here, at the binary —
    /// the kernel ships pure defaults only.
    fn discovery_config(&self) -> DiscoveryConfig {
        let mut cfg = DiscoveryConfig::default();
        if let Some(p) = self.discovery_port {
            cfg.port = p;
        }
        if let Some(g) = self.mcast_group {
            cfg.group = g;
        }
        cfg
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();
    let token = CancellationToken::new();
    let boot_id = Uuid::now_v7();

    // ---- startup pipeline: build-up is sacred (R0.4, L17) -----------------
    let discovery =
        pipeline::step("config", async { Ok::<_, String>(cli.discovery_config()) }).await;
    let http_port = cli.http_port;

    // Identity: minted once, persistent and immutable (D6); poetic default
    // name collision-checked against the room; explicit flag = rename intent.
    let identity = pipeline::step("identity", {
        let discovery = discovery.clone();
        async move { identity::load_or_mint(cli.stone_name.as_deref(), &discovery).await }
    })
    .await;

    // The stone's voice: identity + port + version. The chirp source that
    // speaks it is built AFTER the offerings load (it composes inventory).
    let voice = source::Voice {
        stone_id: identity.stone_id.clone(),
        stone_name: identity.stone_name.clone(),
        http_port,
        moss_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // The stone's offerings: loaded from disk via the store port, adopted
    // split to candidates (ghost prevention, OFFERINGS.md §2).
    let offerings = pipeline::step::<
        Arc<offerings::registry::Registry>,
        String,
        _,
    >("offerings-load", {
        let identity_name = identity.stone_name.clone();
        async move {
            // Offering directories live under MOSS_OFFERINGS_DIR (default
            // ~/.zen-garden/offerings) — the rehydration contract's unit.
            let root = std::env::var_os("MOSS_OFFERINGS_DIR")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("USERPROFILE")
                        .or_else(|| std::env::var_os("HOME"))
                        .map(|h| {
                            std::path::PathBuf::from(h)
                                .join(".zen-garden")
                                .join("offerings")
                        })
                })
                .ok_or_else(|| "no home directory known".to_string())?;
            let store = Arc::new(offerings::directory::DirectoryStore::new(
                OfferingsRoot::new(root).base,
            ));
            let registry = Arc::new(offerings::registry::Registry::new(
                store as Arc<dyn offerings::registry::SnapshotStore>,
            ));
            // ADR-0002 slice 2: legacy records receive ledgered homes.
            registry.derive_missing_allocations();
            tracing::info!(
                stone = %identity_name,
                active = registry.snapshot().len(),
                candidates = registry.candidate_count(),
                "offerings loaded"
            );
            Ok(registry)
        }
    })
    .await;
    // The host's worlds, probed at boot (OFFERINGS.md §4): null always
    // exists; docker registers when its socket answers. MOSS_RUNTIME names
    // the DEFAULT world; naming an absent one aborts loudly (L17).
    let requested = std::env::var("MOSS_RUNTIME").ok();
    let (runtime_registry, default_runtime) = pipeline::step::<
        (Arc<offerings::runtime::RuntimeRegistry>, String),
        String,
        _,
    >("runtime-select", {
        async move {
            // Adopt the worlds that answer (L25): null always exists.
            let mut worlds: Vec<Arc<dyn offerings::runtime::Runtime>> =
                vec![Arc::new(offerings::runtime::NullRuntime)];
            match offerings::docker::DockerRuntime::connect() {
                Ok(d) => worlds.push(Arc::new(d)),
                Err(e) => tracing::info!(error = %e, "docker world not present"),
            }
            let registry = offerings::runtime::RuntimeRegistry::build(worlds);

            // Explicit intent must exist among adopted worlds (L17);
            // otherwise the host default is companion-grade "null".
            let default_kind = requested.clone().unwrap_or_else(|| "null".into());
            registry
                .by_kind(&default_kind)
                .map_err(|e| format!("MOSS_RUNTIME={default_kind}: {e}"))?;
            tracing::info!(default = %default_kind, available = ?registry.kinds(), "runtimes adopted");
            Ok((Arc::new(registry), default_kind))
        }
    })
    .await;

    let ingress = pipeline::step("ingress-bind", {
        let discovery = discovery.clone();
        async move { Ingress::bind(&discovery, Some(discovery.group)).await }
    })
    .await;

    // The offering catalog, layered per ADR-0008: the embedded approved
    // catalog is the floor (the release tagged what this binary places);
    // MOSS_CATALOG_DIR / ~/.zen-garden/catalog adds and overrides BY
    // NAME; the manifests overlay twin is highest. No directory needs to
    // exist — first light is zero-config.
    let catalog_root = std::env::var_os("MOSS_CATALOG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|h| std::path::PathBuf::from(h).join(".zen-garden").join("catalog"))
        });
    let catalog_overlays = std::env::var_os("MOSS_CATALOG_OVERLAY_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|h| std::path::PathBuf::from(h).join(".zen-garden").join("manifests"))
        })
        .into_iter()
        .collect::<Vec<_>>();
    let catalog = pipeline::step::<
        Arc<offerings::manifest::Catalog>,
        String,
        _,
    >("catalog-load", {
        async move {
            // ADR-0008 layering: the embedded approved catalog is the
            // floor; the operator dir and the manifests overlay adjust
            // by name. No directory needs to exist for first light.
            Ok(Arc::new(offerings::manifest::Catalog::load_fully_layered(
                catalog_root.as_deref(),
                &catalog_overlays,
            )))
        }
    })
    .await;

    let (dispatcher, dispatcher_handle) = Dispatcher::new(INGRESS_QUEUE);
    let topology = Arc::new(Topology::new());

    // Self is never ingested: the stone's own id bar its frames from the
    // peers map (ADR-0004 §3) — declare it BEFORE any claim can fire.
    topology.set_self_id(&identity.stone_id);
    // The topology cache claims its types from the dispatcher (R2.9, L22); expiry sweeps
    // on protocol time (the threshold IS the protocol — R2.8).
    topology.claim(&dispatcher, token.clone());
    tokio::spawn(Arc::clone(&topology).run_expiry(
        token.clone(),
        discovery.offline_threshold_secs,
    ));

    // Storage banks (ADR-0005 §8): boot scan recognizes adopted devices
    // (the manifest rides the drive); the watcher keeps reality fresh.
    let storage = Arc::new(offerings::storage::Storage::new());
    storage.reconcile(&offerings::storage::scan_volumes());
    tokio::spawn(offerings::storage::watch_mounts(
        Arc::clone(&storage),
        token.clone(),
    ));

    // The announcer speaks through the same bound port number: a boot SONG
    // (full voice) plus the rich ask, then lean heartbeat chirps and
    // change-songs after (ADR-0004 A2.2). The source is DYNAMIC (S2): it
    // composes the offerings AND banks inventories, bumping their revs on
    // OfferingChanged / storage news (follow_* below).
    let chirp_source = source::DynamicChirpSource::new(
        voice.clone(),
        boot_id.to_string(),
        Arc::clone(&offerings),
        Arc::clone(&storage),
    );
    source::follow_offering_changes(&chirp_source, &offerings, token.clone());
    source::follow_storage_changes(&chirp_source, &storage, token.clone());

    // The living will's runner (ADR-0005 §2): hooks via docker when it
    // answers; loud refusal where no world can run them (companion).
    let hook_runner: Arc<dyn offerings::capture_run::HookRunner> =
        match offerings::docker::DockerRuntime::connect() {
            Ok(d) => Arc::new(d),
            Err(_) => Arc::new(offerings::capture_run::NullHooks),
        };
    let capture_runner = Arc::new(offerings::capture_run::Runner::new(
        Arc::clone(&storage),
        Arc::clone(&hook_runner),
    ));

    let announce_socket = ingress.socket();
    tokio::spawn(announce::run(
        Arc::clone(&announce_socket),
        discovery.group,
        discovery.port,
        chirp_source.clone() as Arc<dyn ChirpSource>,
        identity.stone_name.clone(),
        token.clone(),
    ));

    // Answer others' asks — tell half of ask/tell.
    responder::claim(
        &dispatcher,
        ingress.socket(),
        discovery.group,
        discovery.port,
        chirp_source.clone() as Arc<dyn ChirpSource>,
        token.clone(),
    );

    // Ingestion and dispatch run until cancelled. The counters handle is
    // cloned before the move so posture can serve live numbers (D7).
    let ingest_counters = ingress.counters();
    let dispatch_tx = dispatcher.ingest_tx();
    let ingest_token = token.clone();
    tokio::spawn(async move { ingress.run(ingest_token, dispatch_tx).await });
    tokio::spawn(dispatcher_handle.run(token.clone()));

    // Facts census (OFFERINGS.md §6): contributors fire in parallel at
    // boot; the Converger and compile read the published generation.
    let factsheet = Arc::new(offerings::facts::Factsheet::empty());
    pipeline::step::<(), String, _>("facts-census", {
        let factsheet = Arc::clone(&factsheet);
        let kinds: Vec<String> = runtime_registry
            .kinds()
            .iter()
            .map(|s| s.to_string())
            .collect();
        async move {
            let contributors =
                offerings::facts::builtin_contributors(&kinds);
            let snapshot = factsheet.collect(&contributors).await;
            tracing::info!(generation = snapshot.id, facts = snapshot.facts.len(), "facts census complete");
            Ok(())
        }
    })
    .await;

    // Offering directories: the rehydration contract's physical unit
    // (record + plan + configs + volumes in one place).
    let dirs_root = match std::env::var_os("MOSS_OFFERINGS_DIR").map(std::path::PathBuf::from) {
        Some(p) => Some(p),
        None => std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| std::path::PathBuf::from(h).join(".zen-garden").join("offerings")),
    };
    let dirs_root = match dirs_root {
        Some(p) => p,
        None => {
            tracing::error!("no home directory known; cannot host offering directories");
            pipeline::step::<(), String, _>("offerings-root", async {
                Err::<(), _>("no home directory".to_string())
            })
            .await;
            unreachable!()
        }
    };

    // Jobs outlive nothing silently (L11): a journal beside the
    // offerings, and boot marks what the last process left running.
    let journal_root = dirs_root
        .parent()
        .map(|p| p.join("journal").join("jobs"));

    // The offering application service: registry + worlds + catalog + facts,
    // coordinated (OFFERINGS.md §5/§4). The service pool resolves here so a
    // malformed MOSS_SERVICE_PORT_POOL aborts loudly at startup (L17).
    let pool = pipeline::step::<offerings::ports::Pool, String, _>("port-pool", async {
        match std::env::var("MOSS_SERVICE_PORT_POOL") {
            Ok(v) => offerings::ports::Pool::parse(&v)
                .map_err(|e| format!("MOSS_SERVICE_PORT_POOL={v}: {e}")),
            Err(_) => Ok(offerings::ports::Pool::default()),
        }
    })
    .await;
    let garden = Arc::new(offerings::service::OfferingService::new(
        Arc::clone(&offerings),
        runtime_registry,
        default_runtime.clone(),
        catalog,
        Arc::clone(&factsheet),
        OfferingsRoot::new(dirs_root),
        pool,
        Some(Arc::clone(&hook_runner)),
    ));

    // The Converger: reality chases the stored plans until cancelled.
    tokio::spawn(offerings::converge::run(Arc::clone(&garden), token.clone()));

    // Rehydration moment (OFFERINGS.md): one immediate convergence sweep —
    // if Docker lost everything, this brings offerings back before HTTP.
    pipeline::step::<(), String, _>("rehydrate", {
        let garden = Arc::clone(&garden);
        async move {
            let results = offerings::converge::converge_once(&garden).await;
            let healed = results.iter().filter(|(_, o)| *o == offerings::converge::Outcome::Healed).count();
            // Boot is when ghosts confirm: adopted candidates re-enter the
            // room if their containers are here (OFFERINGS.md §2), and the
            // household's hand-run work is recognized right away.
            let detected = offerings::detect::detect_once(&garden).await;
            tracing::info!(
                checked = results.len(),
                healed,
                adopted_confirmed = detected.confirmed.len(),
                adopted_minted = detected.minted.len(),
                "boot convergence complete"
            );
            Ok(())
        }
    })
    .await;

    // The capture scheduler (ADR-0005 §3: five daily implies daily) walks
    // the placed set against the catalog's declared wills.
    tokio::spawn(offerings::capture_run::run_scheduler(
        Arc::clone(&garden),
        Arc::clone(&capture_runner),
        std::time::Duration::from_secs(offerings::capture_run::CAPTURE_CADENCE_SECS),
        token.clone(),
    ));

    // The jobs tracker: the async contract for every long-running operation.
    let jobs_tracker = journal_root
        .map(jobs::JobTracker::with_journal)
        .unwrap_or_default();

    // The pulse bus (ADR-0013): one typed, seq'd channel of the stone's
    // news; adapters translate existing sources into it.
    let pulse_bus = pulse::Bus::new();
    pipeline::step::<usize, String, _>("jobs-reconcile", {
        let jobs_tracker = jobs_tracker.clone();
        async move { Ok(jobs_tracker.interrupt_stale_running()) }
    })
    .await;

    // HTTP surface, last: the garden answers once it can hear. The same
    // chirp source composes the SelfView — one identity, many mouths (B1).
    let state = Arc::new(http::AppState {
        topology: Arc::clone(&topology),
        dispatcher: dispatcher.clone(),
        ingest_counters: Arc::clone(&ingest_counters),
        garden: Arc::clone(&garden),
        storage: Arc::clone(&storage),
        capture: capture_runner,
        jobs: jobs_tracker.clone(),
        pulse: Arc::new(pulse_bus.clone()),
        chirp_source: chirp_source.clone() as Arc<dyn garden_kernel::announce::ChirpSource>,
        stone_name: identity.stone_name.clone(),
        boot_id,
        started_at: chrono::Utc::now(),
    });
    // The pulse adapters (ADR-0013): translate existing sources into
    // the bus until cancelled. The same Arcs the faces see.
    tokio::spawn(pulse::run(
        pulse_bus,
        pulse::Sources {
            garden: Arc::clone(&garden),
            topology: Arc::clone(&topology),
            jobs: jobs_tracker,
            storage: Arc::clone(&storage),
            dispatcher: dispatcher.clone(),
            ingest: Arc::clone(&ingest_counters),
        },
        token.clone(),
    ));

    let app = http::router(state);
    let listener = pipeline::step("http-listen", async move {
        tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, http_port))
            .await
            .map_err(|e| format!("bind 0.0.0.0:{http_port}: {e}"))
    })
    .await;

    tracing::info!(
        stone = %identity.stone_name,
        group = %discovery.group,
        discovery_port = discovery.port,
        http_port,
        "moss awake"
    );

    // ---- run until signalled ----------------------------------------------
    let shutdown_token = token.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            match tokio::signal::ctrl_c().await {
                Ok(()) => tracing::info!("shutdown signal received"),
                Err(e) => {
                    // No signal channel: graceful shutdown becomes impossible.
                    // Refuse to fake it by cancelling at once; park instead.
                    tracing::error!(error = %e, "signal listener failed; kill to stop");
                    std::future::pending::<()>().await;
                }
            }
            shutdown_token.cancel();
        })
        .await
        .unwrap_or_else(|e| eprintln!("http server error: {e}"));

    // ---- farewell ----------------------------------------------------------
    // Let cancelled tasks wind down, then speak goodbye so peers drop us
    // without waiting out the threshold (PoC parity: 3 copies, debounce-free).
    tokio::time::sleep(Duration::from_millis(SHUTDOWN_GRACE_MS)).await;
    let final_body = chirp_source.body();
    if let Err(e) =
        announce::send_goodbye(&announce_socket, discovery.group, discovery.port, final_body).await
    {
        tracing::warn!(error = %e, "goodbye send failed");
    }
    tracing::info!("goodbye spoken; garden rests");
}

