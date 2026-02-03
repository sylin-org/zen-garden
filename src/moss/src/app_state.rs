//! Application state shared across HTTP handlers
//!
//! Holds all dependencies for moss daemon:
//! - Service registry (Vec<ServiceInfo>)
//! - Docker manager
//! - Manifest registry (unified software/hardware manifests)
//! - Job tracking
//! - Event broadcasting
//! - Hardware capabilities cache
//! - Console printer
//! - mDNS handle for resolution announcements
//!
//! This is the unified AppState used by both main.rs and all API handlers.

use crate::docker::DockerManager;
use crate::domain::{CeremonyRegistry, InfrastructureHandlerRegistry};
use crate::infra::{CeremonyJournal, EventBus, HarvestStore, ManifestRegistry, NurturingStore, SseEvent};
use crate::mdns::MdnsHandle;
use garden_common::console::ConsolePrinter;
use garden_common::storage::SeedBankInfo;
use garden_common::NetworkMetrics;
use crate::tasks::NetworkMonitor;
use garden_common::{HardwareCapabilities, ServiceInfo, StoneResources};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::RwLock;

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
pub use crate::domain::{
    CompiledOffering, OfferingsFingerprint, OfferingsIndexCache,
};

// Offering types (unified and legacy)
pub use garden_common::{
    // Unified offering types
    UnifiedOffering, OfferingModeData, ManagedData, AdoptedData, BorrowedData,
    OfferingStatus, OfferingLocation, OfferingMode,
    // Legacy types (for backward compatibility during migration)
    AdoptedOfferingInfo, BorrowedOfferingInfo,
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

    /// Unified offerings registry (all modes: managed, adopted, borrowed)
    /// Single source of truth for all running offerings
    pub offerings: Arc<RwLock<Vec<UnifiedOffering>>>,

    /// Manifest registry - single source of truth for all manifests
    /// Contains both software (sw) and hardware (hw) manifests
    pub manifest_registry: Arc<ManifestRegistry>,

    /// Docker daemon manager
    pub docker: Arc<DockerManager>,

    /// Background job tracker
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,

    /// SSE event broadcast channel for presence streaming (Firefly, Cricket, etc.)
    pub sse_tx: tokio::sync::broadcast::Sender<SseEvent>,

    /// Domain event bus (unified event dispatch for offerings, storage, stone events)
    pub event_bus: EventBus,

    /// Shutdown coordination channel
    pub shutdown_tx: Arc<tokio::sync::Notify>,

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

    /// Topology cache for discovered stones (in-memory only)
    pub topology_cache: crate::domain::topology::TopologyCache,

    /// Storage routing cache for seed banks across stones (STORAGE-0003)
    pub storage_cache: crate::domain::storage_cache::StorageCache,

    /// Self topology entry (this stone's current state)
    pub self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,

    /// mDNS handle for re-registration on resolution changes (Linux only)
    /// Used when IP/MAC changes to update mDNS service advertisement
    pub mdns_handle: Option<Arc<MdnsHandle>>,

    // === Ceremony Infrastructure ===

    /// Active ceremony registry (in-memory state)
    pub ceremony_registry: Arc<CeremonyRegistry>,

    /// Ceremony journal (persistent state for crash recovery)
    pub ceremony_journal: Arc<CeremonyJournal>,

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

    /// Cached seed bank information (updated on storage events + periodic refresh)
    /// Background task: storage_monitor (events) + health_monitor (disk usage refresh)
    pub seed_bank_cache: Arc<RwLock<Vec<SeedBankInfo>>>,

    /// Cached network metrics (updated every 5s by health_monitor task)
    pub network_metrics_cache: Arc<RwLock<Option<NetworkMetrics>>>,

    /// Subsystem readiness state
    pub subsystems: SubSystems,
}

// ============================================================================
// Subsystem Readiness
// ============================================================================

/// Subsystem readiness tracking
///
/// Background tasks set these flags when subsystems become operational.
/// Consumers check flags before attempting operations that require readiness.
#[derive(Clone)]
pub struct SubSystems {
    /// Network subsystem state
    pub network: NetworkSubSystem,
}

impl Default for SubSystems {
    fn default() -> Self {
        Self {
            network: NetworkSubSystem::default(),
        }
    }
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
    pub async fn persist_offerings(&self) -> anyhow::Result<()> {
        let offerings = self.offerings.read().await;
        crate::infra::save_unified_offerings(&offerings).await
    }

    /// Legacy alias for persist_offerings (backward compatibility)
    pub async fn persist_registry(&self) -> anyhow::Result<()> {
        self.persist_offerings().await
    }

    /// Sync self_entry services from offerings
    ///
    /// Converts UnifiedOffering → TopologyServiceEntry and updates self_entry.
    /// Optionally triggers immediate chirp announcement (if network is ready).
    /// Called after any offerings modification.
    pub async fn sync_self_services(&self, auto_chirp: bool) {
        let offerings = self.offerings.read().await;
        let topology_services = garden_common::TopologyServiceEntry::from_unified_offerings(&offerings);

        {
            let mut entry = self.self_entry.write().await;
            entry.services = topology_services;
            entry.last_seen = chrono::Utc::now();
        }

        tracing::debug!(count = offerings.len(), "Synced self_entry services from offerings");

        if auto_chirp && self.subsystems.network.ready.load(Ordering::Relaxed) {
            let entry = self.self_entry.read().await.clone();
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to auto-chirp after service sync");
            }
        }
    }

    /// Add or update a single offering
    ///
    /// Immediately syncs to self_entry and triggers chirp.
    /// This is the primary method for offering state changes.
    pub async fn upsert_offering(&self, mut offering: UnifiedOffering, auto_chirp: bool) {
        offering.touch();
        {
            let mut offerings = self.offerings.write().await;
            if let Some(pos) = offerings.iter().position(|o| o.offering_id == offering.offering_id) {
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

    /// Legacy: Add or update a single managed service
    ///
    /// Converts ServiceInfo to UnifiedOffering and calls upsert_offering.
    pub async fn upsert_service(&self, service: ServiceInfo, auto_chirp: bool) {
        let offering = UnifiedOffering::from_service_info(service);
        self.upsert_offering(offering, auto_chirp).await;
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

    /// Batch update offerings (for reconciliation)
    ///
    /// Replaces entire offerings registry and triggers chirp.
    pub async fn replace_offerings(&self, offerings: Vec<UnifiedOffering>, auto_chirp: bool) {
        {
            let mut registry = self.offerings.write().await;
            *registry = offerings;
        }

        self.sync_self_services(auto_chirp).await;

        if let Err(e) = self.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after batch update");
        }
    }

    /// Legacy: Batch update managed services
    pub async fn replace_services(&self, services: Vec<ServiceInfo>, auto_chirp: bool) {
        // Convert to UnifiedOfferings, preserving existing non-managed offerings
        let offerings = self.offerings.read().await;
        let mut new_offerings: Vec<UnifiedOffering> = offerings
            .iter()
            .filter(|o| !o.is_managed())
            .cloned()
            .collect();
        drop(offerings);

        for svc in services {
            new_offerings.push(UnifiedOffering::from_service_info(svc));
        }

        self.replace_offerings(new_offerings, auto_chirp).await;
    }

    // ========================================================================
    // Unified Offering Accessors
    // ========================================================================

    /// Get all offerings
    pub async fn get_offerings(&self) -> Vec<UnifiedOffering> {
        self.offerings.read().await.clone()
    }

    /// Get managed offerings only
    pub async fn get_managed_offerings(&self) -> Vec<UnifiedOffering> {
        self.offerings.read().await
            .iter()
            .filter(|o| o.is_managed())
            .cloned()
            .collect()
    }

    /// Get adopted offerings only
    pub async fn get_adopted_offerings(&self) -> Vec<UnifiedOffering> {
        self.offerings.read().await
            .iter()
            .filter(|o| o.is_adopted())
            .cloned()
            .collect()
    }

    /// Get borrowed offerings only
    pub async fn get_borrowed_offerings(&self) -> Vec<UnifiedOffering> {
        self.offerings.read().await
            .iter()
            .filter(|o| o.is_borrowed())
            .cloned()
            .collect()
    }

    /// Find offering by name or offering type
    pub async fn find_offering(&self, name: &str) -> Option<UnifiedOffering> {
        self.offerings.read().await
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(name) || o.offering.eq_ignore_ascii_case(name))
            .cloned()
    }

    /// Find offering by ID
    pub async fn find_offering_by_id(&self, offering_id: &str) -> Option<UnifiedOffering> {
        self.offerings.read().await
            .iter()
            .find(|o| o.offering_id == offering_id)
            .cloned()
    }

    /// Get snapshot of managed services as legacy ServiceInfo
    pub async fn get_services(&self) -> Vec<ServiceInfo> {
        self.offerings.read().await
            .iter()
            .filter(|o| o.is_managed())
            .filter_map(|o| o.to_service_info())
            .collect()
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
            entry.endpoint = new_endpoint;
            entry.mac = new_mac.clone();
            entry.last_seen = chrono::Utc::now();
        }

        // Re-register mDNS with updated MAC (resolution info changed)
        if let Some(ref mdns) = self.mdns_handle {
            if let Err(e) = mdns.reregister(new_mac.as_deref()) {
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
}
