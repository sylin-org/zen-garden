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
use crate::domain::CeremonyRegistry;
use crate::infra::{CeremonyJournal, HarvestStore, ManifestRegistry};
use crate::mdns::MdnsHandle;
use crate::console::ConsolePrinter;
use crate::tasks::NetworkMonitor;
use garden_common::{HardwareCapabilities, ServiceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// SSE event for client notifications
#[derive(Clone, Debug, serde::Serialize)]
pub struct MossEvent {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub job_id: Option<String>,
}

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

// Offering modes types
pub use garden_common::{
    AdoptedOfferingInfo, BorrowedOfferingInfo, OfferingMode,
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

    /// Service registry (persisted to disk)
    /// Vec format for compatibility with existing persistence layer
    pub registry: Arc<RwLock<Vec<ServiceInfo>>>,

    /// Adopted offerings registry (native/existing services)
    pub adopted_offerings: Arc<RwLock<Vec<AdoptedOfferingInfo>>>,

    /// Borrowed offerings registry (external network services)
    pub borrowed_offerings: Arc<RwLock<Vec<BorrowedOfferingInfo>>>,

    /// Manifest registry - single source of truth for all manifests
    /// Contains both software (sw) and hardware (hw) manifests
    pub manifest_registry: Arc<ManifestRegistry>,

    /// Docker daemon manager
    pub docker: Arc<DockerManager>,

    /// Background job tracker
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,

    /// Event broadcast channel for SSE streaming
    pub event_tx: tokio::sync::broadcast::Sender<MossEvent>,

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

    /// Nourishment job status channels (for SSE streaming)
    pub nourishment_jobs: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<String>>>>,

    /// Election service for distributed elections (testing)
    pub election_service: Arc<RwLock<crate::tasks::election_service::ElectionService>>,
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

    /// Persist registry to disk
    ///
    /// Reads the current registry and saves to disk atomically.
    pub async fn persist_registry(&self) -> anyhow::Result<()> {
        let registry = self.registry.read().await;
        crate::infra::save_registry_vec(&registry).await
    }
    
    /// Sync self_entry services from registry
    /// 
    /// Converts ServiceInfo → TopologyServiceEntry and updates self_entry.
    /// Optionally triggers immediate chirp announcement.
    /// Called after any registry modification.
    pub async fn sync_self_services(&self, auto_chirp: bool) {
        let registry = self.registry.read().await;
        let topology_services = garden_common::TopologyServiceEntry::from_service_infos(&registry);
        
        {
            let mut entry = self.self_entry.write().await;
            entry.services = topology_services;
            entry.last_seen = chrono::Utc::now();
        }
        
        tracing::debug!(count = registry.len(), "Synced self_entry services from registry");
        
        if auto_chirp {
            let entry = self.self_entry.read().await.clone();
            if let Err(e) = crate::announcement::announce(&entry).await {
                tracing::warn!(error = ?e, "Failed to auto-chirp after service sync");
            }
        }
    }
    
    /// Add or update a single service in registry and self_entry
    /// 
    /// Immediately syncs to self_entry and triggers chirp.
    /// This is the primary method for service state changes.
    pub async fn upsert_service(&self, service: ServiceInfo, auto_chirp: bool) {
        {
            let mut registry = self.registry.write().await;
            if let Some(pos) = registry.iter().position(|s| s.name == service.name) {
                registry[pos] = service;
            } else {
                registry.push(service);
            }
        }
        
        self.sync_self_services(auto_chirp).await;
        
        if let Err(e) = self.persist_registry().await {
            tracing::error!(error = ?e, "Failed to persist registry after upsert");
        }
    }
    
    /// Remove a service from registry and self_entry
    /// 
    /// Immediately syncs to self_entry and triggers chirp.
    pub async fn remove_service(&self, service_name: &str, auto_chirp: bool) {
        {
            let mut registry = self.registry.write().await;
            registry.retain(|s| s.name != service_name);
        }
        
        self.sync_self_services(auto_chirp).await;
        
        if let Err(e) = self.persist_registry().await {
            tracing::error!(error = ?e, "Failed to persist registry after removal");
        }
    }
    
    /// Batch update services (for reconciliation/adoption)
    /// 
    /// Replaces entire registry and triggers chirp.
    pub async fn replace_services(&self, services: Vec<ServiceInfo>, auto_chirp: bool) {
        {
            let mut registry = self.registry.write().await;
            *registry = services;
        }
        
        self.sync_self_services(auto_chirp).await;
        
        if let Err(e) = self.persist_registry().await {
            tracing::error!(error = ?e, "Failed to persist registry after batch update");
        }
    }
    
    /// Get snapshot of services (read-only)
    pub async fn get_services(&self) -> Vec<ServiceInfo> {
        self.registry.read().await.clone()
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
        let (_, new_mac) = crate::infra::network::get_local_ip_and_mac();

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
