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

use crate::domain::{Offerings, Orchestration, Security, Tool};
use crate::infra::{EventBus, ManifestRegistry, PulseEvent};
use garden_common::console::ConsolePrinter;
use garden_common::tools::ToolDelta;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

    /// Manifest registry - single source of truth for all manifests
    /// Contains both software (sw) and hardware (hw) manifests
    pub manifest_registry: Arc<ManifestRegistry>,

    /// Platform domain — Docker, runtime, network monitor, infrastructure handlers.
    pub platform: Arc<crate::domain::Platform>,

    /// Background job tracker
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,

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

    /// Compiled offerings index (with compatibility checks)
    pub offerings_index: Arc<RwLock<Option<OfferingsIndex>>>,

    /// Console event printer (for tty/systemd/verbose modes)
    pub console: Arc<ConsolePrinter>,

    /// Garden-wide tool registry and delta stream (ARCH-0004).
    pub tool: Arc<Tool>,

    /// Security domain — pond trust, inter-stone TLS, ceremonies (ARCH-0004).
    pub security: Arc<Security>,

    /// Discovery domain — mDNS re-registration handle and Koi embedded handle.
    pub discovery: Arc<crate::domain::Discovery>,

    /// Presence domain — election service and notification registry.
    pub presence: Arc<crate::domain::Presence>,

    /// Companion domain — registry of external companions (Cricket, Firefly, etc.)
    pub companion: Arc<crate::domain::Companion>,

    // === Cached Metrics (updated by background tasks, read-only for endpoints) ===
    // IMPORTANT: These caches exist to keep API endpoints fast (<10ms).
    // Endpoints MUST NOT perform I/O - they read from these caches only.
    // Background tasks are responsible for keeping caches fresh.
    /// Log broadcast channel (for live SSE log streaming)
    pub log: tokio::sync::broadcast::Sender<String>,

    /// Subsystem readiness state
    pub subsystems: SubSystems,

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

impl axum::extract::FromRef<AppState> for Arc<ManifestRegistry> {
    fn from_ref(state: &AppState) -> Self {
        state.manifest_registry.clone()
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

/// Subsystem readiness tracking
///
/// Background tasks set these flags when subsystems become operational.
/// Consumers check flags before attempting operations that require readiness.
#[derive(Clone, Default)]
pub struct SubSystems {
    /// Network subsystem state
    pub network: NetworkSubSystem,
    /// Client subsystem state
    pub docker: DockerSubSystem,
}

/// Network subsystem state
///
/// Tracks whether the network stack is ready for communications.
#[derive(Clone)]
pub struct NetworkSubSystem {
    /// True when a valid LAN IP is detected (not loopback).
    /// Set by Network, read by Announcer/mDNS.
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

/// Client subsystem state
///
/// Tracks whether the Client daemon is available for container operations.
#[derive(Clone)]
pub struct DockerSubSystem {
    /// True when Client daemon is healthy (ping succeeds).
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

    /// Reconcile local tools projection and publish resulting deltas.
    ///
    /// This is the single entry point for publishing local tool updates.
    /// Writes to both the registry (TOOLS-0003) and the legacy tools_cache
    /// until all read sites are migrated.
    pub async fn refresh_local_tools_projection(&self) {
        let projections = crate::domain::tools::projector::project_local_tools(self).await;

        let deltas = {
            let mut reg = self.tool.registry.write().await;
            reg.reconcile_local(
                &self.current.stone.id,
                projections,
                crate::domain::garden_registry::EntryOrigin::Local,
            )
        };

        self.publish_tool_deltas(deltas, true).await;
    }

    /// Ingest remote tools beacon and publish resulting stream deltas locally.
    pub async fn ingest_tools_beacon(&self, beacon: garden_common::tools::ToolsBeacon) {
        let deltas = {
            let mut reg = self.tool.registry.write().await;
            reg.apply_remote_beacon(&beacon)
        };

        self.publish_tool_deltas(deltas, false).await;
    }

    /// Remove all projected tools for a stone (goodbye/offline path).
    pub async fn remove_tools_for_stone(&self, stone_id: &str) {
        let deltas = {
            let mut reg = self.tool.registry.write().await;
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
            let _ = self.tool.delta.send(delta.clone());
        }

        if broadcast_beacon {
            let endpoint = self.current.address.read().await.http_base();
            if endpoint.trim().is_empty() {
                return;
            }
            if let Err(e) = crate::infra::broadcast_tools_beacon(
                &self.current.stone.id,
                &self.current.stone.name,
                &endpoint,
                deltas,
            )
            .await
            {
                tracing::warn!(error = %e, "Failed to broadcast tools beacon");
            }
        }
    }

    /// Build the self topology entry on demand from source domains.
    ///
    /// Replaces the mutable self_entry cache. Reads from:
    /// - current.stone (identity)
    /// - current.address (network)
    /// - current.health (status)
    /// - current.mac (MAC address)
    /// - current.capabilities (hardware)
    /// - offerings (local offerings -> TopologyServiceEntry)
    /// - presence.notifications (tags)
    pub async fn build_self_entry(&self) -> garden_common::TopologyEntry {
        let address = self.current.address.read().await.clone();
        let health = self.current.health.read().await.clone();
        let mac = self.current.mac.read().await.clone();
        let capabilities = self.current.capabilities.read().await.clone();
        let tags = self.presence.notifications.compile();

        // Build services from the active offerings pool. `with_active`
        // bounds the lock scope to the closure — no guard escapes.
        let services = self
            .offerings
            .with_active(garden_common::TopologyServiceEntry::from_offerings)
            .await;

        garden_common::TopologyEntry {
            stone_id: self.current.stone.id.clone(),
            stone_name: self.current.stone.name.clone(),
            address,
            moss_version: crate::version_string(),
            mac,
            health,
            capabilities,
            services,
            status: garden_common::StoneStatus::Online,
            discovered_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            tags,
            gateways: vec![], // TOOLS-0003: registry beacon is the single path
        }
    }

    /// Sync services and optionally chirp.
    ///
    /// With `build_self_entry()` assembling the topology entry on demand,
    /// this method only needs to trigger an immediate chirp when requested.
    /// Called after any offerings modification.
    pub(crate) async fn sync_self_services(&self, auto_chirp: bool) {
        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.build_self_entry().await;
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to auto-chirp after service sync");
            }
        }
    }

    /// Chirp after capabilities change.
    ///
    /// With `build_self_entry()` reading capabilities from `current.capabilities`
    /// directly, this method only needs to trigger a chirp so peers see the update.
    pub(crate) async fn sync_self_capabilities(&self, auto_chirp: bool) {
        tracing::info!("Capabilities updated — build_self_entry will read fresh data");

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.build_self_entry().await;
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to chirp after capabilities sync");
            }
        }
    }

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
            let mut h = self.current.health.write().await;
            *h = health.clone();
        }

        tracing::debug!(health = %health, "Updated stone health");

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.build_self_entry().await;
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to chirp after health update");
            }
        }
    }

    /// Announce resolution change (IP/MAC changed)
    ///
    /// Called when the means to resolve this stone changes (IP address, MAC address).
    /// This is different from service changes - resolution changes require:
    /// 1. Update current.address and current.mac
    /// 2. Re-register mDNS service (updates TXT records and triggers re-announcement)
    /// 3. Send UDP chirp with updated topology entry
    ///
    /// For service-only changes (no resolution change), use `sync_self_services()` instead.
    pub async fn announce_resolution_change(&self, new_ip: &str) {
        let new_endpoint = format!("http://{}:{}", new_ip, self.current.api_port);

        tracing::info!(
            endpoint = %new_endpoint,
            "Announcing resolution change (IP/MAC)"
        );

        // Get fresh MAC address (may have changed with network)
        let (_, new_mac) = garden_common::infra::network::get_local_ip_and_mac();

        // Update current.address and current.mac (source fields)
        {
            let old_tls_port = self.current.address.read().await.tls_port;
            let new_ip: std::net::IpAddr = match new_ip.parse() {
                Ok(ip) => ip,
                Err(e) => {
                    tracing::warn!(raw = %new_ip, error = %e, "Failed to parse new IP — skipping resolution change");
                    return;
                }
            };
            let mut new_addr = garden_common::PeerAddress::new(new_ip, self.current.api_port);
            if let Some(tp) = old_tls_port {
                new_addr = new_addr.with_tls(tp);
            }
            *self.current.address.write().await = new_addr;
            *self.current.mac.write().await = new_mac.clone();
        }

        // Re-register mDNS with updated IP and MAC
        if let Some(ref mdns) = self.discovery.mdns
            && let Err(e) = mdns.reregister(new_ip, new_mac.as_deref()).await
        {
            tracing::warn!(error = ?e, "Failed to re-register mDNS after resolution change");
        }

        // Immediately chirp the updated entry via UDP
        let entry = self.build_self_entry().await;
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
        // This also triggers an incremental tools beacon broadcast via
        // `publish_tool_deltas`, so the garden learns about the change.
        self.refresh_local_tools_projection().await;

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
