//! Test support — minimal AppState and router construction for integration tests.
//!
//! Provides [`build_test_state`] and [`build_test_router`] for in-process API testing
//! through the axum router, without Docker, network, or filesystem side effects.
//!
//! # Usage (integration tests)
//!
//! ```rust,ignore
//! use garden_moss::testing::{build_test_router, build_test_state};
//! use axum::body::Body;
//! use axum::http::{Request, StatusCode};
//! use tower::ServiceExt;
//!
//! #[tokio::test]
//! async fn health_returns_ok() {
//!     let app = build_test_router().await;
//!     let resp = app
//!         .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
//!         .await
//!         .unwrap();
//!     assert_eq!(resp.status(), StatusCode::OK);
//! }
//! ```

use crate::app_state::AppState;
use crate::bootstrap::router;
use crate::domain;
use crate::infra;
use axum::Router;
use garden_common::PeerAddress;
use garden_common::console::{ConsoleMode, ConsolePrinter};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

// ── Noop PlatformRuntime ────────────────────────────────────────────────────

/// A no-op [`PlatformRuntime`] for tests. All methods are silent stubs.
struct NoopRuntime;

impl garden_common::PlatformRuntime for NoopRuntime {
    fn write_line(&self, _text: &str) {}
    fn notify_ready(&self) {}
    fn notify_stopping(&self) {}
    fn notify_watchdog(&self) {}
    fn notify_status(&self, _status: &str, _extend_timeout_usec: Option<u64>) {}
}

// ── Builder ─────────────────────────────────────────────────────────────────

/// Build a minimal [`AppState`] suitable for integration tests.
///
/// The returned state contains:
/// - A synthetic stone identity (`test-stone-id` / `stone-test`)
/// - Empty offerings, capabilities, topology, and resources
/// - Noop console, event bus, pulse, log, subsystems
/// - A Koi handle with all capabilities disabled (no network I/O)
/// - A Docker client connected to the local daemon (health may fail — that is fine)
/// - Real `CeremonyHost`, `CeremonyRegistry`, `CeremonyJournal` (to a temp dir)
/// - Real `CompanionRegistry` (to a temp dir — avoids global paths)
///
/// Callers can mutate the returned `AppState` before passing to `build_router`.
pub async fn build_test_state() -> AppState {
    let shutdown_token = CancellationToken::new();
    let event_bus = infra::EventBus::new();
    let (pulse_tx, _) = tokio::sync::broadcast::channel(64);
    let (log_tx, _) = tokio::sync::broadcast::channel(64);
    let (tool_delta_tx, _) = tokio::sync::broadcast::channel(64);
    let (storage_tick_raw, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageTick>(16);
    let (storage_tick_debounced, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageTick>(16);
    let (storage_changed, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageChanged>(16);
    let (rescan_tx, _rescan_rx) = tokio::sync::mpsc::channel(1);

    // Docker client: attempt connection. If Docker is not available the
    // client is still valid — only health checks and container ops will fail.
    let docker =
        Arc::new(crate::docker::Client::new().expect("Docker client construction should not fail"));

    // Koi handle: all capabilities disabled — no mDNS, DNS, certmesh, proxy, HTTP.
    let koi_handle = Arc::new(
        koi_embedded::Builder::new()
            .service_mode(koi_embedded::ServiceMode::EmbeddedOnly)
            .mdns(false)
            .dns_enabled(false)
            .health(false)
            .certmesh(false)
            .proxy(false)
            .udp(false)
            .http(false)
            .build()
            .expect("Koi builder should succeed with all features disabled")
            .start()
            .await
            .expect("Koi start should succeed with all features disabled"),
    );

    // Temp directories for stores that write to disk.
    let temp = std::env::temp_dir().join("zen-garden-test");
    let harvest_store = Arc::new(infra::HarvestStore::new(&temp));
    let nurturing_store = Arc::new(infra::NurturingStore::new(
        infra::HarvestStore::new(&temp),
        docker.clone(),
    ));
    let harvest_ops = Arc::new(crate::infra::harvest::OsHarvestOps::new(
        docker.clone(),
        harvest_store,
    ));
    let ceremony_registry = Arc::new(domain::CeremonyRegistry::new());
    let ceremony_journal: Arc<dyn domain::CeremonyPersistence + Send + Sync> =
        Arc::new(infra::CeremonyJournal::new(&temp));

    let election_service = Arc::new(crate::tasks::election_service::Elections::new(
        "test-stone-id".to_string(),
        "stone-test".to_string(),
        Box::new(crate::tasks::state_provider::PlaceholderStateProvider),
    ));

    let companion_dir = temp.join("companions");
    let companion_data = temp.join("companion-data");
    let _ = std::fs::create_dir_all(&companion_dir);
    let _ = std::fs::create_dir_all(&companion_data);
    let companion_registry =
        Arc::new(infra::CompanionRegistry::with_path(companion_dir, companion_data).await);

    let test_metrics = Arc::new(domain::Metrics::new());

    let mut subsystems = crate::domain::Subsystems::new(test_metrics.clone()).await;
    subsystems.register("network");
    subsystems.register("docker");
    let subsystems = Arc::new(subsystems);
    let network = crate::tasks::Network::start(subsystems.clone()).await;

    // ManifestRegistry — empty (no runtime manifest directories in tests)
    let manifest_registry = Arc::new(garden_common::manifests::ManifestRegistry {
        sw: garden_common::manifests::OfferingRegistry {
            entries: HashMap::new(),
            categories: Vec::new(),
        },
        hw: garden_common::manifests::HwManifests {
            entries: HashMap::new(),
            vendors: Vec::new(),
        },
    });

    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    let current_address = Arc::new(RwLock::new(PeerAddress::new(loopback, 7185)));

    let infrastructure_handlers = Arc::new(domain::InfrastructureHandlerRegistry::new(Vec::new()));

    // Jobs aggregate (ARCH-0021 Book IV) — ephemeral, empty state.
    let jobs = Arc::new(
        domain::Jobs::with_shared_state(
            Arc::new(RwLock::new(HashMap::new())),
            test_metrics.clone(),
            event_bus.clone(),
        )
        .await,
    );

    // Capabilities handle — shared with Catalog aggregate (ARCH-0022 Book V).
    let capabilities = Arc::new(RwLock::new(None));

    // Catalog aggregate (ARCH-0022 Book V) — persistent, empty registry.
    let catalog = Arc::new(
        domain::Catalog::new(
            manifest_registry.clone(),
            capabilities.clone(),
            Arc::new(domain::FileCatalogCache),
            test_metrics.clone(),
        )
        .await,
    );

    AppState {
        current: Arc::new(domain::Current {
            stone: Arc::new(domain::current::Stone {
                id: "test-stone-id".to_string(),
                name: "stone-test".to_string(),
            }),
            storage: Arc::new(domain::Storage {
                volumes: domain::new_volumes(),
                media: domain::new_media(),
                changed: storage_changed,
            }),
            capabilities,
            hardware_topology: Arc::new(RwLock::new(None)),
            address: current_address,
            health: Arc::new(RwLock::new("thriving".to_string())),
            mac: Arc::new(RwLock::new(None)),
            api_port: 7185,
            resources: Arc::new(domain::current::Resources {
                system: Arc::new(RwLock::new(None)),
                network: Arc::new(RwLock::new(None)),
                gpu: Arc::new(RwLock::new(None)),
            }),
        }),
        metrics: test_metrics.clone(),
        offerings: Arc::new(
            domain::Offerings::new(
                Vec::new(),
                Vec::new(),
                Arc::new(domain::FileOfferingStore),
                test_metrics.clone(),
            )
            .await,
        ),
        catalog,
        platform: Arc::new(domain::Platform {
            docker: docker.clone(),
            runtime: Arc::new(NoopRuntime),
            network: Arc::new(network),
            handlers: infrastructure_handlers,
        }),
        jobs,
        pulse: pulse_tx,
        event_bus,
        shutdown_token: shutdown_token.clone(),
        start_time: Instant::now(),
        console: Arc::new(ConsolePrinter::new(ConsoleMode::Silent)),
        topology: Arc::new(
            domain::topology::Topology::new(
                Arc::new(domain::topology::NoopChirpTransport),
                Arc::new(domain::topology::FileTopologyStore),
                test_metrics.clone(),
            )
            .await,
        ),
        tool: Arc::new(
            domain::Tool::new(
                test_metrics.clone(),
                tool_delta_tx,
                Arc::new(domain::tool::NoopBeaconTransport),
            )
            .await,
        ),
        discovery: Arc::new(
            domain::Discovery::new(koi_handle, None, None, test_metrics.clone()).await,
        ),
        security: Arc::new(
            domain::Security::new(
                Arc::new(AtomicBool::new(false)),
                Arc::new(infra::stone_client::StoneClient::new("stone-test")),
                Arc::new(koi_common::ceremony::CeremonyHost::new(
                    koi_certmesh::pond_ceremony::PondCeremonyRules,
                )),
                ceremony_registry,
                ceremony_journal,
                test_metrics.clone(),
            )
            .await,
        ),
        presence: Arc::new(domain::Presence {
            elections: election_service,
            notifications: Arc::new(garden_common::notifications::NotificationRegistry::new()),
        }),
        companion: Arc::new(domain::Companion {
            registry: companion_registry,
        }),
        log: log_tx,
        health: Arc::new(
            domain::Health::new(
                test_metrics.clone(),
                Arc::new(domain::DockerHealthProbe::new(docker.clone())),
            )
            .await,
        ),
        subsystems: subsystems.clone(),
        orchestration: Arc::new(domain::Orchestration {
            storage: domain::StorageOrchestration {
                tick: domain::orchestration::storage::Tick {
                    raw: storage_tick_raw,
                    debounced: storage_tick_debounced,
                },
                nudge: Arc::new(tokio::sync::Notify::new()),
                rescan: rescan_tx,
                s3_listeners: Arc::new(crate::infra::storage::S3Listeners::new(
                    shutdown_token.clone(),
                )),
            },
            nurturing: domain::orchestration::nurturing::NurturingOrchestration {
                harvest_ops,
                store: nurturing_store,
            },
            nourishment: domain::orchestration::nourishment::NourishmentOrchestration {
                jobs: Arc::new(RwLock::new(HashMap::new())),
            },
        }),
        task_supervisor: Arc::new(RwLock::new(None)),
    }
}

/// Build a fully-wired axum [`Router`] backed by [`build_test_state`].
///
/// This is the entry point for most integration tests — use with
/// `tower::ServiceExt::oneshot` for single-request assertions.
pub async fn build_test_router() -> Router {
    let state = build_test_state().await;
    router::configure(state)
}
