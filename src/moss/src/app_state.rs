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
use garden_common::storage::StorageDetectedInfo;
use garden_common::tools::ToolDelta;
use garden_common::NetworkMetrics;
use garden_common::{HardwareCapabilities, NotificationRegistry, StoneResources};
use garden_common::GatewayRegistration;
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

    /// Storage routing cache for seed banks across stones (STORAGE-0003)
    pub storage_cache: crate::domain::storage_cache::StorageCache,

    /// Unified tools projection cache (offerings + seed-banks)
    pub tools_cache: crate::domain::tools::ToolsCache,

    /// Tools stream broadcast channel (normative automation stream)
    pub tools_tx: tokio::sync::broadcast::Sender<ToolDelta>,

    /// Self topology entry (this stone's current state)
    pub self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,

    /// Gateway registrations from orchestrators (ORCH-0004).
    /// Key: offering name ("ollama"), Value: gateway registration.
    /// One gateway per offering per stone. TTL-evicted during chirp building.
    pub gateways: Arc<RwLock<HashMap<String, GatewayRegistration>>>,

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


    /// Cached candidate devices (empty USB drives ready for preparation)
    /// Background task: storage_monitor (USB events) + periodic refresh
    /// Linux-only; always empty on other platforms
    pub candidates_cache: Arc<RwLock<Vec<StorageDetectedInfo>>>,

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

    /// Mount tracker for seed bank mount persistence (STORAGE-0006)
    /// Shared with coordinator (persistence + hotplug tasks) and release handler.
    /// Prevents the fight-loop where persistence re-mounts a just-released device.
    #[cfg(target_os = "linux")]
    pub mount_tracker: crate::infra::storage::MountTracker,



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

    /// Seed bank lifecycle objects — single source of truth (STORAGE-0007).
    ///
    /// Keyed by seed bank ID (GUIDv7). Each `SeedBank` composes a `StorageDevice`
    /// (mount health) and a `SeedBankStore` (I/O), plus domain state (role, pin).
    ///
    /// Writers: bootstrap (init), coordinator (health tick, hotplug),
    ///          orchestration (role assignment), pin/unpin handlers.
    /// Readers: portrait, beacon builder, nurturing, replication, API handlers.
    pub seed_banks: crate::domain::SeedBanks,
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
    pub async fn refresh_local_tools_projection(&self) {
        let projections = crate::domain::tools::projector::project_local_tools(self).await;
        let deltas = {
            let mut cache = self.tools_cache.write().await;
            cache.reconcile_local(&self.stone_id, projections)
        };

        self.publish_tool_deltas(deltas, true).await;
    }

    /// Ingest remote tools beacon and publish resulting stream deltas locally.
    pub async fn ingest_tools_beacon(&self, beacon: garden_common::tools::ToolsBeacon) {
        let deltas = {
            let mut cache = self.tools_cache.write().await;
            cache.apply_remote_beacon(&beacon)
        };

        self.publish_tool_deltas(deltas, false).await;
    }

    /// Remove all projected tools for a stone (goodbye/offline path).
    pub async fn remove_tools_for_stone(&self, stone_id: &str) {
        let deltas = {
            let mut cache = self.tools_cache.write().await;
            cache.remove_stone_tools(stone_id)
        };
        self.publish_tool_deltas(deltas, false).await;
    }

    async fn publish_tool_deltas(&self, deltas: Vec<ToolDelta>, broadcast_beacon: bool) {
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

        // Collect non-expired gateway registrations (TTL = 60s)
        let gateway_entries: Vec<GatewayRegistration> = {
            let now = chrono::Utc::now();
            let ttl = chrono::Duration::seconds(60);
            let gateways = self.gateways.read().await;
            gateways
                .values()
                .filter(|gw| now.signed_duration_since(gw.registered_at) < ttl)
                .cloned()
                .collect()
        };

        {
            let mut entry = self.self_entry.write().await;
            entry.services = topology_services;
            entry.tags = tags;
            entry.gateways = gateway_entries;
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
            offerings.retain(|o| o.name != service_name);
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
                let dominated = best.get(&o.name).is_some_and(|&prev| {
                    let prev_ts = offerings[prev]
                        .updated_at
                        .unwrap_or(offerings[prev].registered_at);
                    let cur_ts = o.updated_at.unwrap_or(o.registered_at);
                    cur_ts <= prev_ts
                });
                if !dominated {
                    best.insert(o.name.clone(), i);
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
            if let Some(o) = offerings.iter_mut().find(|o| o.name == name) {
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
            .find(|o| o.name.eq_ignore_ascii_case(name))
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
    // Seed Bank Lifecycle (STORAGE-0007)
    // ========================================================================

    /// Refresh `seed_banks` from a registry scan.
    ///
    /// Called by coordinator hotplug/persistence tasks after mount recovery or
    /// device detection. Adds new seed banks, removes departed ones, and
    /// updates health/capacity on existing ones.
    pub async fn refresh_seed_banks_from_scan(
        &self,
        registry: &crate::infra::storage::SeedBankRegistry,
    ) {
        use crate::domain::SeedBank;
        use crate::infra::storage::StorageDevice;

        let scanned = registry.list();

        let mut banks = self.seed_banks.write().await;

        let mut seen_ids: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(scanned.len());

        for info in &scanned {
            seen_ids.insert(info.id.clone());

            if let Some(existing) = banks.get_mut(&info.id) {
                existing.storage.capacity_bytes = info.capacity_bytes;
                existing.storage.used_bytes = info.used_bytes;
                existing.name = info.name.clone();
                existing.visibility = info.visibility;
                if info.online {
                    existing.storage.health =
                        crate::infra::storage::StorageHealth::Healthy;
                }
            } else {
                let manifest_path = std::path::Path::new(&info.mount_path)
                    .join(".zen-garden")
                    .join("manifest.json");
                let manifest = match tokio::fs::read_to_string(&manifest_path).await {
                    Ok(content) => {
                        match serde_json::from_str::<garden_common::storage::SeedBankManifest>(
                            &content,
                        ) {
                            Ok(m) => m,
                            Err(e) => {
                                tracing::warn!(
                                    name = %info.name, error = %e,
                                    "Failed to parse manifest for new seed bank"
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            name = %info.name, error = %e,
                            "Failed to read manifest for new seed bank"
                        );
                        continue;
                    }
                };

                let storage = StorageDevice::from_seed_bank_info(info);
                let bank = SeedBank::from_storage(storage, &manifest, None).await;

                tracing::info!(
                    name = %bank.name,
                    id = %bank.id,
                    "New seed bank lifecycle object created (hotplug)"
                );

                banks.insert(bank.id.clone(), bank);
            }
        }

        banks.retain(|id, bank| {
            if seen_ids.contains(id) {
                true
            } else {
                tracing::info!(
                    name = %bank.name,
                    id = %id,
                    "Seed bank departed — removing lifecycle object"
                );
                false
            }
        });
    }

    // ========================================================================
    // Seed Bank Projections
    // ========================================================================

    /// Snapshot of roles keyed by seed bank name — for beacon/broadcast callers.
    pub async fn seed_bank_roles_snapshot(
        &self,
    ) -> HashMap<String, garden_common::storage::SeedBankRole> {
        let banks = self.seed_banks.read().await;
        banks
            .values()
            .map(|b| (b.name.clone(), b.role))
            .collect()
    }

    /// Snapshot of pins keyed by seed bank name — for beacon/broadcast callers.
    pub async fn seed_bank_pins_snapshot(&self) -> HashMap<String, String> {
        let banks = self.seed_banks.read().await;
        banks
            .values()
            .filter_map(|b| b.pin_id().map(|p| (b.name.clone(), p.to_string())))
            .collect()
    }

    /// Run health ticks on all local seed bank lifecycle objects.
    ///
    /// Called by the coordinator health tick (~10s). Each bank's storage
    /// device is probed for mount liveness and capacity, and domain state
    /// (pin) is reconciled from disk.
    pub async fn tick_seed_bank_health(&self) {
        let mut banks = self.seed_banks.write().await;
        for bank in banks.values_mut() {
            bank.health_tick().await;
        }
    }
}
