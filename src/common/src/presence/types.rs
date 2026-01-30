//! Presence protocol data types (SSE payload contracts)

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Event filter for SSE subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Categories to include (if empty, all categories included)
    pub categories: Vec<String>,
}

impl EventFilter {
    /// Create filter that allows all events
    pub fn allow_all() -> Self {
        Self { categories: Vec::new() }
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
    pub services: Vec<ServiceState>,
    pub timestamp: DateTime<Utc>,
}

/// Stone state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneState {
    pub name: String,
    pub health: String,      // "thriving", "withering", "wilting"
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub uptime_seconds: u64,
    pub pond_active: bool,
}

/// Service state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub name: String,
    pub state: String,       // "running", "stopped", etc.
    pub health: String,      // "healthy", "unhealthy"
}

/// Client-initiated notification (Rake → Moss → Companions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientNotification {
    pub event_type: String,  // "tended", "observed", etc.
    pub client: String,      // "rake", "lantern", etc.
    pub from_host: Option<String>,  // Hostname or IP
    pub message: Option<String>,    // Optional display message
}
