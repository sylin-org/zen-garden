//! Garden topology — aggregated stone state
//!
//! Maintains the in-memory view of all stones registered with Lantern.
//! Uses TopologyEntry from garden_common as the canonical stone representation.
//! API endpoints read from this cache only — no I/O in handlers.

use chrono::{DateTime, Utc};
use garden_common::types::topology::TopologyEntry;
use garden_common::{LanternServiceState, LanternStoneState, LanternTopology, StoneStatus};
use serde::Serialize;
use std::collections::HashMap;

/// Aggregated garden topology held in memory.
///
/// Stores `TopologyEntry` per stone (keyed by stone_id or stone_name).
/// Enrichment data (offerings, seed banks, companions, resources) is stored
/// separately and merged on read by API handlers.
pub struct GardenTopology {
    /// Stones keyed by stone_id (or stone_name if no id)
    pub stones: HashMap<String, TopologyEntry>,
    /// Enrichment data per stone (keyed same as stones)
    pub enrichment: HashMap<String, StoneEnrichment>,
    pub last_updated: DateTime<Utc>,
}

/// Enrichment data fetched from Moss portrait API.
/// Updated by background aggregation task.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StoneEnrichment {
    /// Deterministic HSL color derived from stone_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Full offering list from the stone
    pub offerings: Vec<EnrichedOffering>,
    /// Seed bank information
    pub seed_banks: Vec<EnrichedSeedBank>,
    /// Companion information
    pub companions: Vec<EnrichedCompanion>,
    /// Live system resources (CPU, memory, disk)
    pub resources: Option<EnrichedResources>,
    /// When this enrichment was last refreshed
    pub last_enriched: Option<DateTime<Utc>>,
}

/// Offering data for dashboard display (subset of full Offering)
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedOffering {
    pub offering_id: String,
    pub name: String,
    pub offering: String,
    pub category: String,
    pub status: String,
    pub health: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_name: Option<String>,
}

/// Seed bank data for dashboard display
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedSeedBank {
    pub id: String,
    pub name: String,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub visibility: String,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_id: Option<String>,
}

/// Companion data for dashboard display
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedCompanion {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Live resource metrics for dashboard gauges
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedResources {
    pub cpu_cores: usize,
    pub cpu_percent: f32,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub memory_percent: f32,
    pub disk_total_gb: u64,
    pub disk_used_gb: u64,
    pub disk_percent: f32,
    pub uptime_seconds: u64,
}

impl Default for GardenTopology {
    fn default() -> Self {
        Self::new()
    }
}

impl GardenTopology {
    pub fn new() -> Self {
        Self {
            stones: HashMap::new(),
            enrichment: HashMap::new(),
            last_updated: Utc::now(),
        }
    }

    pub fn stones_online_count(&self) -> usize {
        self.stones
            .values()
            .filter(|s| s.status == StoneStatus::Online)
            .count()
    }

    pub fn stones_total_count(&self) -> usize {
        self.stones.len()
    }

    /// Get all topology entries as a vec (for serialization)
    pub fn all_entries(&self) -> Vec<&TopologyEntry> {
        self.stones.values().collect()
    }

    /// Get a single stone by cache key
    pub fn get_stone(&self, key: &str) -> Option<&TopologyEntry> {
        self.stones.get(key)
    }

    /// Get enrichment data for a stone
    pub fn get_enrichment(&self, key: &str) -> Option<&StoneEnrichment> {
        self.enrichment.get(key)
    }

    /// Convert to the legacy LanternTopology wire type (backward compat)
    pub fn to_lantern_topology(&self) -> LanternTopology {
        let stones: Vec<LanternStoneState> = self
            .stones
            .values()
            .map(|entry| LanternStoneState {
                stone_id: Some(entry.stone_id.clone()),
                name: entry.stone_name.clone(),
                endpoint: entry.endpoint.clone(),
                status: match entry.status {
                    StoneStatus::Online => "online",
                    StoneStatus::Offline => "offline",
                }
                .to_string(),
                services: entry
                    .services
                    .iter()
                    .map(|svc| LanternServiceState {
                        name: svc.name.clone(),
                        service_type: svc.offering.clone(),
                        status: svc.status.clone(),
                        connection_string: String::new(),
                    })
                    .collect(),
                last_seen: entry.last_seen.to_rfc3339(),
                first_seen: entry.discovered_at.to_rfc3339(),
                offline_since: None,
            })
            .collect();

        LanternTopology {
            stones,
            last_updated: self.last_updated.to_rfc3339(),
        }
    }
}
