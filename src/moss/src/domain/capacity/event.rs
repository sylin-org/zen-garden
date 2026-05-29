//! Domain event for the `Capacity` bounded context (STORAGE-0020).

use serde::Serialize;

use super::budget::Pressure;

/// Pressure-level names, for `Metrics::register_domain` per-kind counters.
pub const PRESSURE_KINDS: &[&str] = &["healthy", "elevated", "high", "critical"];

/// Emitted when the governed filesystem's pressure level transitions.
///
/// Fires only on a *change* of level — a steady `Healthy` filesystem is
/// silent. `Clone + Serialize` so the event is itself the SSE wire format
/// with no intermediate mapping (code standards §13).
#[derive(Debug, Clone, Serialize)]
pub struct CapacityChanged {
    /// Pressure before this transition.
    pub from: Pressure,
    /// Pressure after this transition.
    pub to: Pressure,
    /// Filesystem used percentage at the time of transition.
    pub used_percent: f64,
    /// Free bytes on the governed filesystem at the time of transition.
    pub available_bytes: u64,
    /// When the transition was observed.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
