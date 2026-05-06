//! Moss daemon runtime — the central dependency injection container.
//!
//! After ARCH-0017 aggregate extraction, this struct holds `Arc<Aggregate>`
//! fields for each bounded context plus cross-cutting infrastructure
//! (shutdown token, event bus, console). The only method with logic is
//! `emit_storage_changed`, which coordinates across multiple aggregates.

use crate::domain::{
    Catalog, Health, Jobs, Metrics, Offerings, Security, Subsystems, Tool,
};
use crate::domain::orchestration::{NourishmentOrchestration, NurturingOrchestration};
use crate::infra::{EventBus, PulseEvent};
use garden_common::console::ConsolePrinter;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;


/// Moss daemon runtime — central dependency injection container.
///
/// Each domain aggregate is an `Arc<T>` field. Cross-cutting concerns
/// (shutdown token, event bus, console, log channel) live here directly.
/// The only method with logic is [`emit_storage_changed`](Self::emit_storage_changed),
/// which coordinates across multiple bounded contexts.
#[derive(Clone)]
pub struct Moss {
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
    /// **Reads** use the typed query API: `snapshot()`, `find_by_id()`,
    /// `find_by_name()`, `with_active()`, `count_active()`.
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

    /// Health aggregate (ARCH-0024) — per-offering health probing,
    /// transition detection, and event emission. Stateless facade that
    /// delegates probe execution through `HealthProbe` port and offering
    /// mutation through the Offerings aggregate.
    pub health: Arc<Health>,

    /// Subsystem readiness — ARCH-0023 aggregate (Book VI of ARCH-0017)
    pub subsystems: Arc<Subsystems>,

    /// Nurturing infrastructure — A/B backup scheduling and harvest
    /// archives (ARCH-0029: dissolved from Orchestration).
    pub nurturing: Arc<NurturingOrchestration>,

    /// Nourishment SSE channels — per-job broadcast senders for update
    /// progress streaming (ARCH-0029: dissolved from Orchestration).
    pub nourishment: Arc<NourishmentOrchestration>,

    /// Task supervisor status handle (ARCH-0015). Set after supervisor is built.
    pub task_supervisor: Arc<RwLock<Option<crate::tasks::supervisor::SupervisorHandle>>>,
}

// ============================================================================
// FromRef — handler dependency extraction (code standards §6)
// ============================================================================

// Each impl extracts a narrow dependency from Moss. Handlers declare only
// what they need: `State(companion): State<Arc<Companion>>` instead of full Moss.

impl axum::extract::FromRef<Moss> for Arc<crate::domain::Current> {
    fn from_ref(state: &Moss) -> Self {
        state.current.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<crate::domain::Platform> {
    fn from_ref(state: &Moss) -> Self {
        state.platform.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Tool> {
    fn from_ref(state: &Moss) -> Self {
        state.tool.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Offerings> {
    fn from_ref(state: &Moss) -> Self {
        state.offerings.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Metrics> {
    fn from_ref(state: &Moss) -> Self {
        state.metrics.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Jobs> {
    fn from_ref(state: &Moss) -> Self {
        state.jobs.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Security> {
    fn from_ref(state: &Moss) -> Self {
        state.security.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<crate::domain::Discovery> {
    fn from_ref(state: &Moss) -> Self {
        state.discovery.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<crate::domain::Presence> {
    fn from_ref(state: &Moss) -> Self {
        state.presence.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<crate::domain::Companion> {
    fn from_ref(state: &Moss) -> Self {
        state.companion.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<NurturingOrchestration> {
    fn from_ref(state: &Moss) -> Self {
        state.nurturing.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<NourishmentOrchestration> {
    fn from_ref(state: &Moss) -> Self {
        state.nourishment.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<Catalog> {
    fn from_ref(state: &Moss) -> Self {
        state.catalog.clone()
    }
}

impl axum::extract::FromRef<Moss> for EventBus {
    fn from_ref(state: &Moss) -> Self {
        state.event_bus.clone()
    }
}

impl axum::extract::FromRef<Moss> for CancellationToken {
    fn from_ref(state: &Moss) -> Self {
        state.shutdown_token.clone()
    }
}

impl axum::extract::FromRef<Moss> for Arc<ConsolePrinter> {
    fn from_ref(state: &Moss) -> Self {
        state.console.clone()
    }
}

// ============================================================================
// Cross-cutting coordination (STORAGE-0013)
// ============================================================================

impl Moss {
    /// Emit a storage domain event.
    ///
    /// Coordinates across multiple bounded contexts: bridges the event to
    /// `EventBus` (so `PulseDomainBridge` translates for SSE), sends on the
    /// dedicated `StorageChanged` broadcast channel (infra subscribers),
    /// triggers an immediate tools projection refresh, and nudges
    /// orchestration so role resolution reacts to storage changes.
    ///
    /// This method is genuinely cross-cutting and cannot live in any single
    /// aggregate — it stays on the root struct.
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
        // remote stones learn about the change.
        crate::domain::tool::projection::reproject_and_publish(self).await;

        // Nudge orchestration so role resolution (Primary/Replica) reacts
        // immediately to connect/disconnect/role changes (STORAGE-0018).
        self.current.storage.coordination.nudge.notify_one();
    }
}
