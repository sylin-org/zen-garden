//! Domain events for the `Subsystems` bounded context.

use serde::Serialize;

/// Change kinds emitted by the `Subsystems` aggregate.
///
/// Only **interesting transitions** fire events — setting a subsystem
/// to ready when it is already ready is a no-op.
#[derive(Debug, Clone, Serialize)]
pub enum ChangeKind {
    /// A subsystem transitioned from not-ready to ready.
    Ready { name: String },
    /// A subsystem transitioned from ready to not-ready.
    Unready { name: String, reason: String },
}

impl ChangeKind {
    /// Stable name for Metrics per-kind counter lookup.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ready { .. } => "Ready",
            Self::Unready { .. } => "Unready",
        }
    }

    /// All kind names for Metrics `register_domain` with kinds.
    pub const ALL_NAMES: &'static [&'static str] = &["Ready", "Unready"];
}

/// Domain event emitted when a subsystem's readiness changes.
#[derive(Debug, Clone, Serialize)]
pub struct SubsystemsChanged {
    /// What changed.
    pub kind: ChangeKind,
    /// When the change occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
