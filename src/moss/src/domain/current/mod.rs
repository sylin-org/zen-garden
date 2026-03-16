//! Current domain — this stone's identity, storage, and runtime state.
//!
//! `state.current.*` = writable stores owned by this stone.
//! `state.*`         = garden-wide aggregates (local + remote peers).

use std::sync::Arc;
use tokio::sync::RwLock;
use garden_common::{HardwareCapabilities, StoneResources};

/// This stone's identity — set at startup, effectively immutable at runtime.
///
/// Named `Stone` so call sites read `state.current.stone.id`, `state.current.stone.name`.
#[derive(Debug, Clone)]
pub struct Stone {
    /// Permanent cryptographic/install identity — never changes.
    pub id: String,
    /// User-assigned display name — fixed for the lifetime of the process.
    pub name: String,
}

/// Topology sub-context (`state.current.topology`).
#[derive(Clone)]
pub struct Topology {
    /// In-memory topology cache for discovered stones.
    pub cache: crate::domain::topology::TopologyCache,
    /// Dirty flag for topology persistence (TOPO-0002).
    pub dirty: crate::domain::topology::TopologyDirtyFlag,
    /// Self topology entry (this stone's current state).
    pub self_entry: Arc<RwLock<garden_common::TopologyEntry>>,
}

/// Current domain context (`state.current`).
///
/// Groups all state that describes *this* stone — its identity, local storage,
/// topology view, hardware capabilities, and runtime metrics.
#[derive(Clone)]
pub struct Current {
    /// This stone's identity (id and name).
    pub stone: Arc<Stone>,

    /// This stone's storage (volumes, media, domain event channel).
    pub storage: Arc<crate::domain::Storage>,

    /// Topology state: cache, dirty flag, and this stone's self-entry.
    pub topology: Topology,

    /// Hardware capabilities cache (detected at startup, persisted).
    pub capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,

    /// API port for constructing endpoint URLs.
    pub api_port: u16,

    /// System metrics cache (CPU/memory/disk, updated every 5s).
    pub system_resources: Arc<RwLock<Option<StoneResources>>>,
}
