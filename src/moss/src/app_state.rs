//! Application state shared across HTTP handlers
//!
//! Holds all dependencies for moss daemon:
//! - Offerings registry (Vec<Offering>)
//! - Docker manager
//! - Manifest registry (unified software/hardware manifests)
//! - Job tracking
//! - Event broadcasting
//! - Hardware capabilities cache
//! - Console printer
//! - mDNS handle for resolution announcements
//! - Notification registry for cross-stone awareness tags
//!
//! This is the unified AppState used by both main.rs and all API handlers.

use crate::docker::DockerManager;
use crate::domain::{CeremonyRegistry, InfrastructureHandlerRegistry};
use crate::infra::{
    stone_client::StoneClient, CeremonyJournal, EventBus, HarvestStore, ManifestRegistry,
    NurturingStore, PulseEvent,
};
use crate::mdns::MdnsHandle;
use crate::tasks::NetworkMonitor;
use garden_common::console::ConsolePrinter;
use garden_common::PlatformRuntime;
use garden_common::tools::ToolDelta;
use garden_common::NetworkMetrics;
use garden_common::{HardwareCapabilities, NotificationRegistry, StoneResources};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Job execution status
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Background job for tracking long-running operations
#[derive(Clone, Debug, serde::Serialize)]
pub struct Job {
    pub id: String,
    pub offerings: Vec<String>,
    pub status: JobStatus,
    pub completed: Vec<String>,
    pub failed: HashMap<String, String>, // service -> error message
    pub started_at: std::time::SystemTime,
    pub completed_at: Option<std::time::SystemTime>,
}

// Offerings types moved to domain/offerings.rs
pub use crate::domain::{CompiledOffering, OfferingsFingerprint, OfferingsIndexCache};

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
    /// Unique stone identifier (GUID v7, immutable once generated)
    pub stone_id: String,

    /// Stone identity (e.g., "stone-01", hostname)
    pub stone_name: String,

    /// Unified offerings registry (all modes: managed, adopted, borrowed).
    ///
    /// **Write access**: use gateway methods only (`update_offering`,
    /// `update_offering_by_name`, `update_offerings_batch`, `upsert_offering`,
    /// `remove_offering`, `remove_service`, `replace_offerings`).
    /// Direct `.write()` is reserved for `app_state.rs` internals.
    /// **Read access**: `.read()` is fine from anywhere.
    pub offerings: Arc<RwLock<Vec<Offering>>>,

    /// Manifest registry - single source of truth for all manifests
    /// Contains both software (sw) and hardware (hw) manifests
    pub manifest_registry: Arc<ManifestRegistry>,

    /// Docker daemon manager
    pub docker: Arc<DockerManager>,

    /// Background job tracker
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,

    /// Unified pulse event channel (domain + transport events).
    /// Consumers: pulse stream (full firehose), presence stream (domain-only, translated).
    pub pulse_tx: tokio::sync::broadcast::Sender<PulseEvent>,

    /// Domain event bus (unified event dispatch for offerings, storage, stone events)
    pub event_bus: EventBus,

    /// Cooperative shutdown token (MOSS-0004: phased shutdown)
    /// Cancel this to signal all background tasks, SSE streams, and servers to stop.
    /// This is the SINGLE source of truth for shutdown. OS signals, deploy handlers,
    /// and admin API all cancel this token; everything cascades from there.
    pub shutdown_token: CancellationToken,

    /// Daemon start time (for uptime calculation)
    pub start_time: Instant,

    /// Compiled offerings index (with compatibility checks)
    pub offerings_index: Arc<RwLock<Option<OfferingsIndexCache>>>,

    /// Console event printer (for tty/systemd/verbose modes)
    pub console: Arc<ConsolePrinter>,

    /// Platform runtime — console/ribbon output and lifecycle signals (ARCH-0002).
    /// Single injection point; no `#[cfg]` above bootstrap/run.rs.
    pub runtime: Arc<dyn PlatformRuntime>,

    /// Hardware capabilities cache (detected at startup, cached to disk)
    pub capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,

    /// Network monitor for IP change detection
    pub network_monitor: Arc<NetworkMonitor>,

    /// API port for constructing endpoint URLs
    pub api_port: u16,

    /// Topology cache for discovered stones (in-memory, persisted via dirty flag)
    pub topology_cache: crate::domain::topology::TopologyCache,

    /// Dirty flag for topology persistence (TOPO-0002)
    /// Set by mutation functions, cleared after flush to disk.
    pub topology_dirty: crate::domain::topology::TopologyDirtyFlag,

    /// Tools stream broadcast channel (normative automation stream)
    pub tools_tx: tokio::sync::broadcast::Sender<ToolDelta>,

    /// Unified garden registry — single source of truth for offerings,
    /// gateways, and storage (TOOLS-0003).
    pub registry: crate::domain::garden_registry::GardenRegistry,

    /// Self topology entry (this stone's current state)
    pub self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,

    /// mDNS handle for re-registration on resolution changes
    /// Used when IP/MAC changes to update mDNS service advertisement
    pub mdns_handle: Option<Arc<MdnsHandle>>,

    /// Koi embedded handle — provides mDNS, DNS, certmesh, proxy, and health capabilities
    /// Shared across all subsystems; sub-handles accessed via `koi_handle.mdns()`, `.dns()`, etc.
    pub koi_handle: Arc<koi_embedded::KoiHandle>,

    /// Pond domain surface — enrollment state and cornerstone identity.
    /// Properties: `enrolled()`, `cornerstone()`.
    /// Mutations trigger `PondEvent::EnrollmentChanged` on the EventBus.
    pub pond: crate::domain::PondState,

    /// Pond active flag — true when certmesh CA is initialized and unlocked.
    /// Cached for fast checks (chirp signing, HTTPS routing). Updated by pond handlers.
    pub pond_active: Arc<std::sync::atomic::AtomicBool>,

    /// HTTPS listener started flag — guards against double-binding :7183.
    /// Set true after the first successful HTTPS bind (boot or dynamic).
    pub https_started: Arc<std::sync::atomic::AtomicBool>,

    /// Stone-to-stone HTTP client gateway.
    /// Automatically upgrades to HTTPS+mTLS when pond certs are available.
    /// Call `stone_client.reload_tls()` after enrollment changes.
    pub stone_client: Arc<StoneClient>,

    // === Ceremony Infrastructure ===
    /// Active ceremony registry (in-memory state)
    pub ceremony_registry: Arc<CeremonyRegistry>,

    /// Ceremony journal (persistent state for crash recovery)
    pub ceremony_journal: Arc<CeremonyJournal>,

    /// Pond ceremony host — drives pond init/join/unlock ceremonies
    /// using the koi-common ceremony protocol.
    pub pond_ceremony_host:
        Arc<koi_common::ceremony::CeremonyHost<koi_certmesh::pond_ceremony::PondCeremonyRules>>,

    /// Harvest store (backup manifests and archives)
    pub harvest_store: Arc<HarvestStore>,

    /// Nurturing store (A/B local backup slots)
    pub nurturing_store: Arc<NurturingStore>,

    /// Nourishment job status channels (for SSE streaming)
    pub nourishment_jobs: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<String>>>>,

    /// Election service for distributed elections (testing)
    pub election_service: Arc<crate::tasks::election_service::ElectionService>,

    /// System metrics cache (CPU/memory/disk usage, updated every 5s)
    pub system_resources: Arc<RwLock<Option<StoneResources>>>,

    /// Companion registry (external Companions like Cricket, Firefly)
    pub companion_registry: Arc<crate::infra::CompanionRegistry>,

    /// Infrastructure handlers for garden-wide effects (registry trust, DNS, etc.)
    /// Handlers react to topology changes and configure local infrastructure.
    pub infrastructure_handlers: Arc<InfrastructureHandlerRegistry>,

    // === Cached Metrics (updated by background tasks, read-only for endpoints) ===
    // IMPORTANT: These caches exist to keep API endpoints fast (<10ms).
    // Endpoints MUST NOT perform I/O - they read from these caches only.
    // Background tasks are responsible for keeping caches fresh.


    /// Cached network metrics (updated every 5s by health_monitor task)
    pub network_metrics_cache: Arc<RwLock<Option<NetworkMetrics>>>,

    /// Cached GPU utilization percentage (FIREFLY-0003)
    /// Updated every 5s by metrics_collector. None = no GPU or query failed.
    pub gpu_utilization: Arc<RwLock<Option<f32>>>,

    // === Notification Registry ===
    // Subsystems register their state (opportunity/attention) here.
    // Tags are compiled and included in topology chirps for cross-stone awareness.
    /// Notification registry for cross-stone awareness tags
    /// Background tasks set/clear notifications, chirp task compiles to tags.
    /// See: garden_common::notifications for source keys and tag types.
    pub notifications: Arc<NotificationRegistry>,

    /// Log broadcast channel (for live SSE log streaming)
    pub log_tx: tokio::sync::broadcast::Sender<String>,

    /// Subsystem readiness state
    pub subsystems: SubSystems,


    /// Storage replication tick channel — **raw** (STORAGE-0006 Phase 4)
    /// Primary seed-bank stores emit `StorageTick` on every write/delete.
    /// Internal only — consumed by the aggregator task, not by downstream consumers.
    pub storage_tick_tx: tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,

    /// Storage tick channel — **aggregated** (STORAGE-0006 Phase 4f)
    /// Per-seed-bank quantized ticks (2 s quiet / 10 s deadline cap).
    /// Subscribers: SSE `/api/v1/stone/storage/stream`, replication task.
    pub storage_agg_tx: tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,

    /// Orchestration nudge — wakes the seed-bank orchestration loop immediately.
    /// Fired when a storage beacon arrives, or after rename/pin/unpin, so role
    /// resolution doesn't have to wait for the next 3-second tick.
    pub orchestration_nudge: Arc<tokio::sync::Notify>,

    /// Unified volume collection (STORAGE-0011) — keyed by device path.
    ///
    /// Single source of truth for all local storage volumes (Spaces).
    /// Populated by `initial_scan()` at boot, kept current by the volume watcher.
    pub volumes: crate::domain::Volumes,

    /// Physical storage media (STORAGE-0011) — keyed by OS device ID.
    ///
    /// Host-only. Detects physical disks including those without partitions
    /// or drive letters. Used for candidate discovery and `storage add`.
    pub media: crate::domain::Media,

    /// Signal to request a volume rescan (STORAGE-0011).
    ///
    /// API handlers send on this after mutating on-disk state (e.g. writing
    /// a manifest during `storage add`). The volume watcher loop listens and
    /// triggers a full reconcile through the existing detection pipeline.
    pub volume_rescan_tx: tokio::sync::mpsc::Sender<()>,

    /// Storage domain event channel (STORAGE-0013).
    ///
    /// Emitted by storage mutation operations (add, remove, rename, role change,
    /// health change, rescan). Subscribers react by pulling fresh state from
    /// AppState boundary methods — the event is a notification, not data carrier.
    ///
    /// Consumers: tools projector, cloud filter, beacon, watcher reconciler,
    /// coordinator, metrics collector, API SSE streams.
    pub storage_changed_tx: tokio::sync::broadcast::Sender<garden_common::storage::StorageChanged>,
}

// ============================================================================
// Subsystem Readiness
// ============================================================================

/// Subsystem readiness tracking
///
/// Background tasks set these flags when subsystems become operational.
/// Consumers check flags before attempting operations that require readiness.
#[derive(Clone, Default)]
pub struct SubSystems {
    /// Network subsystem state
    pub network: NetworkSubSystem,
    /// Docker subsystem state
    pub docker: DockerSubSystem,
}

/// Network subsystem state
///
/// Tracks whether the network stack is ready for communications.
#[derive(Clone)]
pub struct NetworkSubSystem {
    /// True when a valid LAN IP is detected (not loopback).
    /// Set by NetworkMonitor, read by Announcer/mDNS.
    /// Use `ready.load(Ordering::Relaxed)` to check.
    pub ready: Arc<AtomicBool>,
}

impl Default for NetworkSubSystem {
    fn default() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Docker subsystem state
///
/// Tracks whether the Docker daemon is available for container operations.
#[derive(Clone)]
pub struct DockerSubSystem {
    /// True when Docker daemon is healthy (ping succeeds).
    /// Set by DockerMonitor, read by API handlers and background tasks.
    /// Use `ready.load(Ordering::Relaxed)` to check.
    pub ready: Arc<AtomicBool>,
}

impl Default for DockerSubSystem {
    fn default() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AppState {
    /// Request the volume watcher to re-scan and re-classify all volumes.
    ///
    /// Non-blocking. If the channel is full (a rescan is already pending),
    /// the request is silently dropped — one rescan is sufficient.
    pub fn request_volume_rescan(&self) {
        let _ = self.volume_rescan_tx.try_send(());
    }

    /// Get stone ID (GUID v7)
    pub fn stone_id(&self) -> &str {
        &self.stone_id
    }

    /// Get stone name
    pub fn stone_name(&self) -> &str {
        &self.stone_name
    }

    /// Persist offerings to disk
    ///
    /// Reads the current offerings and saves to disk atomically.
    pub(crate) async fn persist_offerings(&self) -> anyhow::Result<()> {
        let offerings = self.offerings.read().await;
        crate::infra::save_offerings(&offerings).await?;
        drop(offerings);

        // Offerings persistence is the canonical mutation boundary for offering state.
        // Reconcile the tools projection immediately so automation consumers get
        // deterministic updates without polling.
        self.refresh_local_tools_projection().await;

        Ok(())
    }

    /// Reconcile local tools projection and publish resulting deltas.
    ///
    /// This is the single entry point for publishing local tool updates.
    /// Writes to both the registry (TOOLS-0003) and the legacy tools_cache
    /// until all read sites are migrated.
    pub async fn refresh_local_tools_projection(&self) {
        let projections = crate::domain::tools::projector::project_local_tools(self).await;

        let deltas = {
            let mut reg = self.registry.write().await;
            reg.reconcile_local(
                &self.stone_id,
                projections,
                crate::domain::garden_registry::EntryOrigin::Local,
            )
        };

        self.publish_tool_deltas(deltas, true).await;
    }

    /// Ingest remote tools beacon and publish resulting stream deltas locally.
    pub async fn ingest_tools_beacon(&self, beacon: garden_common::tools::ToolsBeacon) {
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.apply_remote_beacon(&beacon)
        };

        self.publish_tool_deltas(deltas, false).await;
    }

    /// Remove all projected tools for a stone (goodbye/offline path).
    pub async fn remove_tools_for_stone(&self, stone_id: &str) {
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.remove_stone(stone_id)
        };

        self.publish_tool_deltas(deltas, false).await;
    }

    /// Publish tool deltas to SSE subscribers and optionally broadcast a UDP
    /// tools beacon so remote stones' registries get the update.
    pub async fn publish_tool_deltas(&self, deltas: Vec<ToolDelta>, broadcast_beacon: bool) {
        if deltas.is_empty() {
            return;
        }

        for delta in &deltas {
            let _ = self.tools_tx.send(delta.clone());
        }

        if broadcast_beacon {
            let endpoint = self.self_entry.read().await.address.http_base();
            if endpoint.trim().is_empty() {
                return;
            }
            if let Err(e) = crate::infra::broadcast_tools_beacon(
                &self.stone_id,
                &self.stone_name,
                &endpoint,
                deltas,
            )
            .await
            {
                tracing::warn!(error = %e, "Failed to broadcast tools beacon");
            }
        }
    }

    /// Sync self_entry services and tags from offerings and notifications
    ///
    /// Converts Offering → TopologyServiceEntry and updates self_entry.
    /// Also compiles notification tags for cross-stone awareness.
    /// Optionally triggers immediate chirp announcement (if network is ready).
    /// Called after any offerings modification.
    pub(crate) async fn sync_self_services(&self, auto_chirp: bool) {
        let offerings = self.offerings.read().await;
        let mut topology_services =
            garden_common::TopologyServiceEntry::from_offerings(&offerings);

        // Fix up categories from the compiled offerings index.
        // Offering doesn't carry its category, so from_offering() falls back to the
        // offering name. Patch with the real category from the registry so that
        // chirps carry the correct value and protocol inference works on peers.
        if let Some(index) = self.offerings_index.read().await.as_ref() {
            for svc in &mut topology_services {
                if let Some(compiled) = index.offerings.iter().find(|c| c.name == svc.offering) {
                    svc.category = compiled.category.clone();
                }
            }
        }

        // Compile notification tags for cross-stone awareness
        let tags = self.notifications.compile();

        // TOOLS-0003: Gateways are no longer carried in chirps.
        // They propagate via the tools beacon / registry path exclusively.

        {
            let mut entry = self.self_entry.write().await;
            entry.services = topology_services;
            entry.tags = tags;
            entry.gateways = vec![]; // Empty — registry beacon is the single path
            entry.last_seen = chrono::Utc::now();
        }

        tracing::debug!(
            count = offerings.len(),
            "Synced self_entry services from offerings"
        );

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.self_entry.read().await.clone();
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to auto-chirp after service sync");
            }
        }
    }

    /// Sync self_entry capabilities from the capabilities cache.
    ///
    /// Called after background hardware detection completes to ensure
    /// chirps carry the freshly-detected hardware data instead of the
    /// stale skeleton/cache loaded at boot.
    pub(crate) async fn sync_self_capabilities(&self, auto_chirp: bool) {
        let caps = self.capabilities.read().await.clone();

        {
            let mut entry = self.self_entry.write().await;
            entry.capabilities = caps;
            entry.last_seen = chrono::Utc::now();
        }

        tracing::info!("Synced self_entry capabilities from background detection");

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.self_entry.read().await.clone();
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to chirp after capabilities sync");
            }
        }
    }

    /// Add or update a single offering
    ///
    /// Immediately syncs to self_entry and triggers chirp.
    /// This is the primary method for offering state changes.
    ///
    /// Dedup guard: matches by `offering_id` first, then by FQN (`name`).
    /// This prevents duplicate entries when callers generate a fresh GUID
    /// for an offering that already exists in the registry.
    pub async fn upsert_offering(&self, mut offering: Offering, auto_chirp: bool) {
        offering.touch();
        {
            let mut offerings = self.offerings.write().await;
            if let Some(pos) = offerings
                .iter()
                .position(|o| o.offering_id == offering.offering_id)
            {
                // Exact ID match — update in place
                offerings[pos] = offering;
            } else if let Some(pos) = offerings
                .iter()
                .position(|o| o.name == offering.name)
            {
                // FQN match — same service, different ID (e.g. re-adoption)
                tracing::info!(
                    name = %offering.name,
                    old_id = %offerings[pos].offering_id,
                    new_id = %offering.offering_id,
                    "upsert_offering: FQN already exists, updating in place"
                );
                offerings[pos] = offering;
            } else {
                offerings.push(offering);
            }
        }

        self.sync_self_services(auto_chirp).await;

        if let Err(e) = self.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after upsert");
        }
    }

    /// Remove an offering by ID
    ///
    /// Immediately syncs to self_entry and triggers chirp.
    pub async fn remove_offering(&self, offering_id: &str, auto_chirp: bool) {
        {
            let mut offerings = self.offerings.write().await;
            offerings.retain(|o| o.offering_id != offering_id);
        }

        self.sync_self_services(auto_chirp).await;

        if let Err(e) = self.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after removal");
        }
    }

    /// Remove an offering by name
    ///
    /// Immediately syncs to self_entry and triggers chirp.
    pub async fn remove_service(&self, service_name: &str, auto_chirp: bool) {
        {
            let mut offerings = self.offerings.write().await;
            offerings.retain(|o| o.name.to_string() != service_name);
        }

        self.sync_self_services(auto_chirp).await;

        if let Err(e) = self.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after removal");
        }
    }

    /// Coalesce duplicate offerings by FQN (name)
    ///
    /// When multiple entries share the same `name`, keeps the one that
    /// was most recently updated (or registered). This is a self-heal
    /// mechanism for registries that accumulated duplicates before the
    /// FQN dedup guard was added to `upsert_offering`.
    ///
    /// Returns the number of duplicates removed.
    pub async fn coalesce_duplicate_offerings(&self) -> usize {
        let removed = {
            let mut offerings = self.offerings.write().await;
            let before = offerings.len();

            // Build a map of FQN → best index (most recently updated)
            let mut best: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (i, o) in offerings.iter().enumerate() {
                let key = o.name.to_string();
                let dominated = best.get(&key).is_some_and(|&prev| {
                    let prev_ts = offerings[prev]
                        .updated_at
                        .unwrap_or(offerings[prev].registered_at);
                    let cur_ts = o.updated_at.unwrap_or(o.registered_at);
                    cur_ts <= prev_ts
                });
                if !dominated {
                    best.insert(key, i);
                }
            }

            let keep: std::collections::HashSet<usize> = best.into_values().collect();
            let mut idx = 0usize;
            offerings.retain(|_| {
                let k = keep.contains(&idx);
                idx += 1;
                k
            });

            before - offerings.len()
        };

        if removed > 0 {
            tracing::warn!(
                removed,
                "Coalesced duplicate offerings by FQN"
            );
            self.sync_self_services(true).await;
            if let Err(e) = self.persist_offerings().await {
                tracing::error!(error = ?e, "Failed to persist offerings after coalesce");
            }
        }

        removed
    }

    /// Batch update offerings (for reconciliation)
    ///
    /// Replaces entire offerings registry and triggers chirp.
    pub async fn replace_offerings(&self, offerings: Vec<Offering>, auto_chirp: bool) {
        {
            let mut registry = self.offerings.write().await;
            *registry = offerings;
        }

        self.sync_self_services(auto_chirp).await;

        if let Err(e) = self.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after batch update");
        }
    }

    // ========================================================================
    // Offering Gateway Methods
    // ========================================================================

    /// Update a single offering by ID via a closure.
    ///
    /// This is the preferred way to mutate an existing offering's operational
    /// state (status, health, port, role, etc.).  The closure receives `&mut Offering`
    /// and returns `true` if it made changes.
    ///
    /// After a successful mutation, `self_entry` is synced automatically so
    /// chirps carry current data.  Pass `auto_chirp = true` for immediate
    /// broadcast (status changes) or `false` to let the periodic announcer
    /// pick it up (detail-only changes).
    pub async fn update_offering<F>(
        &self,
        offering_id: &str,
        auto_chirp: bool,
        mutator: F,
    ) -> bool
    where
        F: FnOnce(&mut Offering) -> bool,
    {
        let changed = {
            let mut offerings = self.offerings.write().await;
            if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
                mutator(o)
            } else {
                false
            }
        };

        if changed {
            self.sync_self_services(auto_chirp).await;
        }

        changed
    }

    /// Update a single offering by name (FQN) via a closure.
    ///
    /// Same semantics as `update_offering` but looks up by `offering.name`.
    pub async fn update_offering_by_name<F>(
        &self,
        name: &str,
        auto_chirp: bool,
        mutator: F,
    ) -> bool
    where
        F: FnOnce(&mut Offering) -> bool,
    {
        let changed = {
            let mut offerings = self.offerings.write().await;
            if let Some(o) = offerings.iter_mut().find(|o| o.name.to_string() == name) {
                mutator(o)
            } else {
                false
            }
        };

        if changed {
            self.sync_self_services(auto_chirp).await;
        }

        changed
    }

    /// Batch-update offerings via a closure over the entire vec.
    ///
    /// The closure receives `&mut Vec<Offering>` and returns the count of
    /// offerings it changed.  If > 0, self_entry is synced and offerings
    /// are persisted to disk automatically.
    ///
    /// Use this for bulk operations like the health monitor's iterate-all
    /// pattern where acquiring/releasing the lock per-offering is wasteful.
    pub async fn update_offerings_batch<F>(
        &self,
        mutator: F,
        auto_chirp: bool,
    ) -> usize
    where
        F: FnOnce(&mut Vec<Offering>) -> usize,
    {
        let changed = {
            let mut offerings = self.offerings.write().await;
            mutator(&mut offerings)
        };

        if changed > 0 {
            self.sync_self_services(auto_chirp).await;
            if let Err(e) = self.persist_offerings().await {
                tracing::error!(error = ?e, "Failed to persist after batch update");
            }
        }

        changed
    }

    // ========================================================================
    // Offering Accessors
    // ========================================================================

    /// Get all offerings
    pub async fn get_offerings(&self) -> Vec<Offering> {
        self.offerings.read().await.clone()
    }

    /// Get managed offerings only
    pub async fn get_managed_offerings(&self) -> Vec<Offering> {
        self.offerings
            .read()
            .await
            .iter()
            .filter(|o| o.is_managed())
            .cloned()
            .collect()
    }

    /// Get adopted offerings only
    pub async fn get_adopted_offerings(&self) -> Vec<Offering> {
        self.offerings
            .read()
            .await
            .iter()
            .filter(|o| o.is_adopted())
            .cloned()
            .collect()
    }

    /// Get borrowed offerings only
    pub async fn get_borrowed_offerings(&self) -> Vec<Offering> {
        self.offerings
            .read()
            .await
            .iter()
            .filter(|o| o.is_borrowed())
            .cloned()
            .collect()
    }

    /// Find offering by instance name (FQN)
    pub async fn find_offering(&self, name: &str) -> Option<Offering> {
        self.offerings
            .read()
            .await
            .iter()
            .find(|o| o.name.to_string().eq_ignore_ascii_case(name))
            .cloned()
    }

    /// Find offering by ID
    pub async fn find_offering_by_id(&self, offering_id: &str) -> Option<Offering> {
        self.offerings
            .read()
            .await
            .iter()
            .find(|o| o.offering_id == offering_id)
            .cloned()
    }

    /// Update stone health and immediately chirp
    ///
    /// Use this when stone-level status changes (not just services).
    /// Examples: nourishing starts, nourishing completes, degraded → thriving.
    ///
    /// # Parameters
    /// - `health`: New health status (use constants: STONE_THRIVING, STONE_NOURISHING, etc.)
    /// - `auto_chirp`: If true, broadcasts updated state immediately (if network is ready)
    pub async fn update_stone_health(&self, health: String, auto_chirp: bool) {
        {
            let mut entry = self.self_entry.write().await;
            entry.health = health.clone();
            entry.last_seen = chrono::Utc::now();
        }

        tracing::debug!(health = %health, "Updated stone health");

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.self_entry.read().await.clone();
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to chirp after health update");
            }
        }
    }

    /// Announce resolution change (IP/MAC changed)
    ///
    /// Called when the means to resolve this stone changes (IP address, MAC address).
    /// This is different from service changes - resolution changes require:
    /// 1. Update self_entry with new endpoint and MAC
    /// 2. Re-register mDNS service (updates TXT records and triggers re-announcement)
    /// 3. Send UDP chirp with updated topology entry
    ///
    /// For service-only changes (no resolution change), use `sync_self_services()` instead.
    pub async fn announce_resolution_change(&self, new_ip: &str) {
        let new_endpoint = format!("http://{}:{}", new_ip, self.api_port);

        tracing::info!(
            endpoint = %new_endpoint,
            "Announcing resolution change (IP/MAC)"
        );

        // Get fresh MAC address (may have changed with network)
        let (_, new_mac) = garden_common::infra::network::get_local_ip_and_mac();

        // Update self_entry with new endpoint and MAC
        {
            let mut entry = self.self_entry.write().await;
            let old_tls_port = entry.address.tls_port;
            let new_ip: std::net::IpAddr = new_ip
                .parse()
                .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
            let mut new_addr = garden_common::PeerAddress::new(new_ip, self.api_port);
            if let Some(tp) = old_tls_port {
                new_addr = new_addr.with_tls(tp);
            }
            entry.address = new_addr;
            entry.mac = new_mac.clone();
            entry.last_seen = chrono::Utc::now();
        }

        // Re-register mDNS with updated IP and MAC
        if let Some(ref mdns) = self.mdns_handle {
            if let Err(e) = mdns.reregister(new_ip, new_mac.as_deref()).await {
                tracing::warn!(error = ?e, "Failed to re-register mDNS after resolution change");
            }
        }

        // Immediately chirp the updated entry via UDP
        let entry = self.self_entry.read().await.clone();
        if let Err(e) = crate::announcement::announce(&entry).await {
            tracing::warn!(error = ?e, "Failed to chirp after resolution change");
        } else {
            tracing::info!("Resolution change announced (mDNS + UDP chirp)");
        }
    }

    /// Recover incomplete ceremonies from previous run
    ///
    /// Called on startup to detect ceremonies that were interrupted
    /// (e.g., by crash or restart). Returns count of recovered ceremonies.
    pub async fn recover_ceremonies(&self) -> anyhow::Result<usize> {
        let incomplete = self.ceremony_journal.load_active().await?;
        let count = incomplete.len();

        for ceremony in incomplete {
            tracing::warn!(
                ceremony_id = %ceremony.id,
                ceremony_type = ceremony.ceremony_type.name(),
                state = ?ceremony.state,
                "Found incomplete ceremony from previous run"
            );
            self.ceremony_registry.insert(ceremony).await;
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
    // Storage Service
    // ========================================================================

    /// Create a `StorageService` scoped to this stone's state.
    ///
    /// Cheap to construct (borrows only). Use per-request in handlers
    /// instead of reimplementing resolution/routing logic.
    pub fn storage_service(&self) -> crate::domain::StorageService<'_> {
        crate::domain::StorageService::new(
            &self.volumes,
            &self.registry,
            &self.stone_id,
            Some(&self.storage_tick_tx),
        )
    }

    // ========================================================================
    // Storage Events (STORAGE-0013)
    // ========================================================================

    /// Emit a storage domain event.
    ///
    /// Subscribers (beacon, cloud filter, watcher, coordinator, projector)
    /// react by pulling fresh state from AppState boundary methods.
    /// Also triggers an immediate tools projection refresh so the registry
    /// stays coherent with storage state.
    pub async fn emit_storage_changed(&self, event: garden_common::storage::StorageChanged) {
        tracing::debug!(event = ?event, "Storage domain event");
        let _ = self.storage_changed_tx.send(event);

        // Storage mutations affect the tools projection (seed-bank entries).
        // Refresh immediately so registry consumers see the change without polling.
        self.refresh_local_tools_projection().await;
    }

    /// Subscribe to storage domain events.
    ///
    /// Returns a broadcast receiver. Callers should `select!` on this alongside
    /// their shutdown token. Missed events (lagged receiver) are non-fatal —
    /// subscribers should do a full reconcile on lag.
    pub fn subscribe_storage_changed(
        &self,
    ) -> tokio::sync::broadcast::Receiver<garden_common::storage::StorageChanged> {
        self.storage_changed_tx.subscribe()
    }

    /// Broadcast a storage beacon to the garden.
    ///
    /// Reads current volumes, roles, and pins from the AppState boundary,
    /// then sends a UDP STORAGE_BEACON announcement. This is the single
    /// codepath for beacon broadcasting — callers should not inline this logic.
    pub async fn broadcast_storage_beacon(&self) {
        let endpoint = self.self_entry.read().await.address.http_base();
        let roles = crate::domain::storage::roles_snapshot(&self.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&self.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &self.stone_id,
            &self.stone_name,
            &endpoint,
            &self.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            tracing::warn!(error = %e, "Failed to broadcast storage beacon");
        }
    }

}
