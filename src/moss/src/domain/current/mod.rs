//! Current domain — this stone's identity, storage, and runtime state.
//!
//! `state.current.*` = writable stores owned by this stone.
//! `state.*`         = garden-wide aggregates (local + remote peers).

use garden_common::{HardwareCapabilities, NetworkMetrics, PeerAddress, StoneResources};
use std::sync::Arc;
use tokio::sync::RwLock;

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

/// Runtime metrics — updated by background metrics_collector task.
/// Individual locks: GPU queries are slow (spawn_blocking), disk ticks
/// do partial updates of system_resources.storage.
#[derive(Clone)]
pub struct Metrics {
    /// CPU, memory, disk, uptime (updated every 5s fast tick + 30s disk tick)
    pub system: Arc<RwLock<Option<StoneResources>>>,
    /// Network RX/TX bytes and rates (updated every 5s)
    pub network: Arc<RwLock<Option<NetworkMetrics>>>,
    /// GPU utilization percentage (updated every 5s via spawn_blocking)
    pub gpu: Arc<RwLock<Option<f32>>>,
}

/// Topology sub-context (`state.current.topology`).
#[derive(Clone)]
pub struct Topology {
    /// In-memory topology cache for discovered stones.
    pub cache: crate::domain::topology::TopologyCache,
    /// Dirty flag for topology persistence (TOPO-0002).
    pub dirty: crate::domain::topology::TopologyDirtyFlag,
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

    /// Topology state: peer cache and dirty flag for persistence.
    pub topology: Topology,

    /// Hardware capabilities cache — Tier 1 (detected at startup, persisted).
    pub capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,

    /// Hardware topology cache — Tier 2 (background probe, delta-cached, ARCH-0014).
    pub hardware_topology: Arc<RwLock<Option<garden_common::types::hardware_topology::HardwareTopology>>>,

    /// This stone's network address (updated on IP change).
    pub address: Arc<RwLock<PeerAddress>>,
    /// This stone's health status.
    pub health: Arc<RwLock<String>>,
    /// This stone's MAC address.
    pub mac: Arc<RwLock<Option<String>>>,

    /// API port for constructing endpoint URLs.
    pub api_port: u16,

    /// Runtime metrics (system, network, GPU) — updated by metrics_collector task.
    pub metrics: Arc<Metrics>,
}
