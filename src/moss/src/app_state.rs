//! Application state shared across HTTP handlers
//!
//! Holds all dependencies for moss daemon:
//! - Offerings registry (Vec<Offering>)
//! - Client manager
//! - Manifest registry (unified software/hardware manifests)
//! - Job tracking
//! - Event broadcasting
//! - Hardware capabilities cache
//! - Console printer
//! - mDNS handle for resolution announcements
//! - Notification registry for cross-stone awareness tags
//!
//! This is the unified AppState used by both main.rs and all API handlers.

use crate::domain::{Catalog, Jobs, Metrics, Offerings, Orchestration, Security, Subsystems, Tool};
use crate::infra::{EventBus, PulseEvent};
use garden_common::console::ConsolePrinter;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

// Job value objects moved to `domain/jobs/entry.rs` in Book IV Ch2 of
// ARCH-0017. Re-exported here so `use crate::{Job, JobStatus}` at the
// crate root keeps resolving for call sites during the Ch3–Ch5
// migration.
pub use crate::domain::jobs::{Job, JobStatus};

// Offerings types moved to domain/catalog/ in Book V (ARCH-0022)
pub use crate::domain::{CompiledOffering, OfferingsFingerprint, OfferingsIndex};

// Offering types (unified)
pub use garden_common::{
    AdoptedData, BorrowedData, ManagedData, Offering, OfferingLocation, OfferingMode,
    OfferingModeData, OfferingStatus,
};

/// Application state for HTTP handlers
///
/// This is the central dependency injection container for moss.
/// All fields are wrapped in Arc for cheap cloning across tasks.
#[derive(Clone)]
pub struct AppState {
    /// Current domain — this stone's identity, local storage, topology, capabilities, resources.
    pub current: Arc<crate::domain::Current>,

    /// Offerings aggregate (ARCH-0016) — owns the active pool and the
    /// adopted-candidates pool as a single DDD aggregate.
    ///
    /// **Mutation** goes through methods on the aggregate
    /// (`upsert`, `remove`, `promote`, `demote`, `update`, ...). Every
    /// mutation persists and emits an `OfferingsChanged` event through the
    /// aggregate's broadcast channel.
    ///
    /// **Reads** continue to use `.read().await` during the strangler-vine
    /// migration phase — `Offerings::read()` returns an `ActiveGuard` that
    /// derefs to `&Vec<Offering>`. New code should prefer `snapshot()`,
    /// `find_by_id()`, `with_active()`, or the other typed query methods.
    pub offerings: Arc<Offerings>,

    /// Metrics aggregate (ARCH-0018) — stone self-observation. Holds
    /// per-domain counters, per-task observability data, and global
    /// counters. Hot-path recording is lock-free via `Arc<DomainMetrics>`
    /// clones and atomic counters. Other domain aggregates inject this
    /// at construction and call `record_domain_event` /
    /// `record_mutation_latency` from their `finalize` pipelines.
    pub metrics: Arc<Metrics>,

    /// Catalog aggregate (ARCH-0022) — compiled offerings index, frozen
    /// manifest registry queries, and hardware manifest lookups. Typed
    /// commands (`load`, `rebuild`) replace the legacy free functions.
    /// Absorbs `manifest_registry` and `offerings_index` — those fields
    /// are deleted as of Ch5 of Book V.
    pub catalog: Arc<Catalog>,

    /// Platform domain — Docker, runtime, network monitor, infrastructure handlers.
    pub platform: Arc<crate::domain::Platform>,

    /// Jobs aggregate (ARCH-0021) — typed command/query API for the
    /// `Jobs` bounded context. Every mutation emits `JobsChanged`
    /// (internal) + `JobEvent` (wire) atomically through
    /// `EventBus`. Terminal jobs are swept by the `JobsReaperTask`
    /// background task after the terminal TTL
    /// ([`DEFAULT_TERMINAL_TTL`](crate::domain::JOBS_DEFAULT_TERMINAL_TTL)).
    pub jobs: Arc<Jobs>,

    /// Unified pulse event channel (domain + transport events).
    /// Consumers: pulse stream (full firehose), presence stream (domain-only, translated).
    pub pulse: tokio::sync::broadcast::Sender<PulseEvent>,

    /// Domain event bus (unified event dispatch for offerings, storage, stone events)
    pub event_bus: EventBus,

    /// Cooperative shutdown token (MOSS-0004: phased shutdown)
    /// Cancel this to signal all background tasks, SSE streams, and servers to stop.
    /// This is the SINGLE source of truth for shutdown. OS signals, deploy handlers,
    /// and admin API all cancel this token; everything cascades from there.
    pub shutdown_token: CancellationToken,

    /// Daemon start time (for uptime calculation)
    pub start_time: Instant,

    /// Console event printer (for tty/systemd/verbose modes)
    pub console: Arc<ConsolePrinter>,

    /// Garden-wide tool registry and delta stream (ARCH-0004).
    pub tool: Arc<Tool>,

    /// Garden topology — peer cache, chirp transport, persistence store
    /// DDD aggregate for the Topology bounded context (ARCH-0020).
    /// Owns the peer cache + dirty flag internally, plus the injected
    /// `ChirpTransport` and `TopologyStore` ports.
    pub topology: Arc<crate::domain::topology::Topology>,

    /// Security domain — pond trust, inter-stone TLS, ceremonies (ARCH-0004).
    pub security: Arc<Security>,

    /// Discovery domain — mDNS re-registration handle and Koi embedded handle.
    pub discovery: Arc<crate::domain::Discovery>,

    /// Presence domain — election service and notification registry.
    pub presence: Arc<crate::domain::Presence>,

    /// Companion domain — registry of external companions (Cricket, Firefly, etc.)
    pub companion: Arc<crate::domain::Companion>,

    // === Cross-cutting infrastructure (updated by background tasks) ===
    // API endpoints MUST NOT perform I/O — they read from caches only.
    // Background tasks keep these state slices fresh.
    /// Log broadcast channel (for live SSE log streaming)
    pub log: tokio::sync::broadcast::Sender<String>,

    /// Subsystem readiness — ARCH-0023 aggregate (Book VI of ARCH-0017)
    pub subsystems: Arc<Subsystems>,

    /// Orchestration coordination plane — tick signals, nudge, rescan,
    /// nurturing stores, nourishment job channels (ARCH-0004).
    pub orchestration: Arc<Orchestration>,

    /// Task supervisor status handle (ARCH-0015). Set after supervisor is built.
    pub task_supervisor: Arc<RwLock<Option<crate::tasks::supervisor::SupervisorHandle>>>,
}

// ============================================================================
// FromRef — handler dependency extraction (code standards §6)
// ============================================================================

// Each impl extracts a narrow dependency from AppState. Handlers declare only
// what they need: `State(companion): State<Arc<Companion>>` instead of full AppState.

impl axum::extract::FromRef<AppState> for Arc<crate::domain::Current> {
    fn from_ref(state: &AppState) -> Self {
        state.current.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::domain::Platform> {
    fn from_ref(state: &AppState) -> Self {
        state.platform.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Tool> {
    fn from_ref(state: &AppState) -> Self {
        state.tool.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Offerings> {
    fn from_ref(state: &AppState) -> Self {
        state.offerings.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Metrics> {
    fn from_ref(state: &AppState) -> Self {
        state.metrics.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Jobs> {
    fn from_ref(state: &AppState) -> Self {
        state.jobs.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Security> {
    fn from_ref(state: &AppState) -> Self {
        state.security.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::domain::Discovery> {
    fn from_ref(state: &AppState) -> Self {
        state.discovery.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::domain::Presence> {
    fn from_ref(state: &AppState) -> Self {
        state.presence.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::domain::Companion> {
    fn from_ref(state: &AppState) -> Self {
        state.companion.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Orchestration> {
    fn from_ref(state: &AppState) -> Self {
        state.orchestration.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<Catalog> {
    fn from_ref(state: &AppState) -> Self {
        state.catalog.clone()
    }
}

impl axum::extract::FromRef<AppState> for EventBus {
    fn from_ref(state: &AppState) -> Self {
        state.event_bus.clone()
    }
}

impl axum::extract::FromRef<AppState> for CancellationToken {
    fn from_ref(state: &AppState) -> Self {
        state.shutdown_token.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<ConsolePrinter> {
    fn from_ref(state: &AppState) -> Self {
        state.console.clone()
    }
}

// ============================================================================
// Subsystem Readiness
// ============================================================================

impl AppState {
    /// Subscribe to the live log stream.
    ///
    /// Returns a broadcast receiver of log lines for SSE streaming.
    pub fn log_stream(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.log.subscribe()
    }

    /// Subscribe to the unified pulse event stream.
    ///
    /// Returns a broadcast receiver of [`PulseEvent`] (domain + transport events).
    /// Consumers: pulse SSE (full firehose), presence SSE (domain-only, translated).
    pub fn pulse_stream(&self) -> tokio::sync::broadcast::Receiver<PulseEvent> {
        self.pulse.subscribe()
    }

    /// Request the volume watcher to re-scan and re-classify all volumes.
    ///
    /// Non-blocking. If the channel is full (a rescan is already pending),
    /// the request is silently dropped — one rescan is sufficient.
    pub fn request_volume_rescan(&self) {
        let _ = self.orchestration.storage.rescan.try_send(());
    }

    /// Get stone ID (GUID v7)
    pub fn stone_id(&self) -> &str {
        &self.current.stone.id
    }

    /// Get stone name
    pub fn stone_name(&self) -> &str {
        &self.current.stone.name
    }

    // Self-entry construction and chirp methods moved to the Topology
    // aggregate in ARCH-0020 Book III. Composition helpers at
    // `crate::domain::topology::composition::*` assemble `SelfEntryInputs`
    // from AppState and delegate to the aggregate's typed commands.

    // ========================================================================
    // Offering Accessors — thin delegates to the Offerings aggregate (ARCH-0016)
    // ========================================================================
    //
    // Mutation methods are gone — use `state.offerings.{upsert,remove,update,
    // promote,demote,...}` directly. The aggregate owns the persist+emit
    // invariant internally. See ARCH-0016.

    /// Snapshot of the active offerings pool.
    pub async fn get_offerings(&self) -> Vec<Offering> {
        self.offerings.snapshot().await
    }

    /// Get managed offerings only.
    pub async fn get_managed_offerings(&self) -> Vec<Offering> {
        self.offerings
            .with_active(|o| {
                o.iter()
                    .filter(|o| o.is_managed())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await
    }

    /// Get adopted offerings only.
    pub async fn get_adopted_offerings(&self) -> Vec<Offering> {
        self.offerings
            .with_active(|o| {
                o.iter()
                    .filter(|o| o.is_adopted())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await
    }

    /// Get borrowed offerings only.
    pub async fn get_borrowed_offerings(&self) -> Vec<Offering> {
        self.offerings
            .with_active(|o| {
                o.iter()
                    .filter(|o| o.is_borrowed())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await
    }

    /// Find offering by instance name (FQN).
    pub async fn find_offering(&self, name: &str) -> Option<Offering> {
        self.offerings.find_by_name(name).await
    }

    /// Find offering by ID.
    pub async fn find_offering_by_id(&self, offering_id: &str) -> Option<Offering> {
        self.offerings.find_by_id(offering_id).await
    }

    // The stone-health and resolution-change methods have moved to
    // `crate::domain::topology::composition::*` per ARCH-0020 Book III.

    /// Recover incomplete ceremonies from previous run
    ///
    /// Called on startup to detect ceremonies that were interrupted
    /// (e.g., by crash or restart). Returns count of recovered ceremonies.
    pub async fn recover_ceremonies(&self) -> anyhow::Result<usize> {
        let incomplete = self.security.pond.ceremony.journal.load_active().await?;
        let count = incomplete.len();

        for ceremony in incomplete {
            tracing::warn!(
                ceremony_id = %ceremony.id,
                ceremony_type = ceremony.ceremony_type.name(),
                state = ?ceremony.state,
                "Found incomplete ceremony from previous run"
            );
            self.security.pond.ceremony.registry.insert(ceremony).await;
        }

        if count > 0 {
            tracing::warn!(
                count,
                "Recovered incomplete ceremonies - manual intervention may be required"
            );
        }

        Ok(count)
    }

    // ========================================================================
    // Storage Events (STORAGE-0013)
    // ========================================================================

    /// Emit a storage domain event.
    ///
    /// Subscribers (beacon, cloud filter, watcher, coordinator, projector)
    /// react by pulling fresh state from AppState boundary methods.
    /// Also emits through EventBus so PulseDomainBridge translates the event
    /// for SSE consumers, and triggers an immediate tools projection refresh
    /// so the registry stays coherent with storage state.
    pub async fn emit_storage_changed(&self, event: garden_common::storage::StorageChanged) {
        tracing::debug!(event = ?event, "Storage domain event");

        // Bridge to EventBus so PulseDomainBridge sees storage events.
        let storage_event = crate::domain::events::StorageEvent::from(&event);
        self.event_bus.emit(storage_event);

        // Dedicated broadcast channel for infra subscribers (console, cloud filter, S3, etc.)
        let _ = self.current.storage.changed.send(event);

        // Storage mutations affect the tools projection (seed-bank entries).
        // Refresh immediately so registry consumers see the change without polling.
        // The helper re-projects, reconciles into the Tool aggregate, and
        // publishes any wire deltas via the injected beacon transport so
        // remote stones learn about the change. Storage will emit its own
        // domain events in Book VIII; until then this imperative edge is
        // the explicit coupling between the two bounded contexts.
        crate::domain::tool::projection::reproject_and_publish(self).await;

        // Nudge orchestration so role resolution (Primary/Dormant) reacts
        // immediately to connect/disconnect/role changes (STORAGE-0018).
        self.orchestration.storage.nudge.notify_one();
    }

    /// Subscribe to storage domain events.
    ///
    /// Returns a broadcast receiver. Callers should `select!` on this alongside
    /// their shutdown token. Missed events (lagged receiver) are non-fatal —
    /// subscribers should do a full reconcile on lag.
    pub fn subscribe_storage_changed(
        &self,
    ) -> tokio::sync::broadcast::Receiver<garden_common::storage::StorageChanged> {
        self.current.storage.changed.subscribe()
    }
}
