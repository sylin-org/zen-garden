//! Presence protocol data types (SSE payload contracts)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event filter for SSE subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Categories to include (if empty, all categories included)
    pub categories: Vec<String>,
}

impl EventFilter {
    /// Create filter that allows all events
    pub fn allow_all() -> Self {
        Self {
            categories: Vec::new(),
        }
    }

    /// Check if event category is allowed
    pub fn allows(&self, category: &str) -> bool {
        self.categories.is_empty() || self.categories.iter().any(|c| c == category)
    }
}

/// Snapshot sent on SSE connect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceSnapshot {
    pub stone: StoneState,
    pub offerings: Vec<OfferingState>,
    pub timestamp: DateTime<Utc>,
}

/// Stone state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneState {
    pub name: String,
    pub health: String, // "thriving", "withering", "wilting"
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub uptime_seconds: u64,
    pub pond_active: bool,

    // --- FIREFLY-0003: Extended resource gauges ---
    /// Aggregate disk I/O utilization (0–100)
    #[serde(default)]
    pub io_percent: f64,
    /// GPU compute utilization (0–100), 0 if no GPU
    #[serde(default)]
    pub gpu_percent: f64,
    /// Network receive rate (bytes/sec)
    #[serde(default)]
    pub net_rx_bytes_per_sec: u64,
    /// Network transmit rate (bytes/sec)
    #[serde(default)]
    pub net_tx_bytes_per_sec: u64,

    // --- FIREFLY-0003: Capability flags ---
    /// Any GPU hardware detected
    #[serde(default)]
    pub has_gpu: bool,
    /// GPU utilization above activity threshold
    #[serde(default)]
    pub gpu_active: bool,
    /// This stone runs the Lantern registry
    #[serde(default)]
    pub is_lantern: bool,
    /// Cricket audio companion connected
    #[serde(default)]
    pub has_cricket: bool,

    // --- FIREFLY-0003: Environment ---
    /// Local time as decimal hour (14.5 = 2:30 PM)
    #[serde(default)]
    pub hour: f64,

    // --- FIREFLY-0003: Seed bank summary ---
    /// Seed bank capacity (only present if a seed bank is plugged in)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub seed_bank: Option<SeedBankSummary>,
}

/// Seed bank storage summary for presence protocol (FIREFLY-0003)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankSummary {
    pub name: String,
    pub used_gb: u64,
    pub total_gb: u64,
}

/// Offering state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingState {
    pub name: String,
    pub status: String, // "running", "stopped", "dormant", etc.
    pub health: String, // "healthy", "unhealthy"
}

/// Client-initiated notification (Rake → Moss → Companions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientNotification {
    pub event_type: String,        // "tended", "observed", etc.
    pub client: String,            // "rake", "lantern", etc.
    pub from_host: Option<String>, // Hostname or IP
    pub message: Option<String>,   // Optional display message
}

/// Payload for stone.load.updated events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneLoadUpdatedPayload {
    #[serde(default, alias = "cpu")]
    pub cpu_percent: f64,
    #[serde(default, alias = "memory")]
    pub memory_percent: f64,

    // --- FIREFLY-0003: Extended load fields ---
    #[serde(default)]
    pub disk_percent: f64,
    #[serde(default)]
    pub io_percent: f64,
    #[serde(default)]
    pub gpu_percent: f64,
    #[serde(default)]
    pub gpu_active: bool,
    #[serde(default)]
    pub net_rx_bytes_per_sec: u64,
    #[serde(default)]
    pub net_tx_bytes_per_sec: u64,
}

/// Payload for stone.health.changed events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneHealthChangedPayload {
    pub health: String,
    #[serde(default)]
    pub cpu_percent: f64,
    #[serde(default)]
    pub memory_percent: f64,
}
