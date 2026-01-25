//! Topology types - Stone discovery and network topology
//!
//! Shared types for representing discovered stones and their services.
//! Used by moss for topology cache and by rake for displaying garden state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{HardwareCapabilities, StoneStatus, TopologyServiceEntry};

/// Discovered stone entry
///
/// Represents a stone in the garden network topology.
/// Used for:
/// - Moss: In-memory topology cache of peer stones
/// - Rake: Displaying garden topology via API
/// - P2P: Chirp payload (stones broadcast their TopologyEntry)
///
/// Health progression: starting → initializing → thriving/degraded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEntry {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String,
    pub moss_version: String,
    /// Services running on this stone (lightweight topology representation)
    pub services: Vec<TopologyServiceEntry>,
    /// MAC address for Wake-on-LAN support
    pub mac: Option<String>,
    /// Health status: use health_status constants (STARTING, INITIALIZING, THRIVING, DEGRADED)
    pub health: String,
    /// Hardware capabilities - available after detection (None during early boot)
    pub capabilities: Option<HardwareCapabilities>,
    /// Current connectivity status
    pub status: StoneStatus,
    pub discovered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}
