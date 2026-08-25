//! Health domain events.

use chrono::{DateTime, Utc};
use garden_common::ServiceHealthStatus;
use serde::Serialize;

/// Broadcast event for interesting health transitions.
#[derive(Debug, Clone, Serialize)]
pub struct HealthChanged {
    pub kind: HealthChangeKind,
    pub offering: String,
    pub old_health: ServiceHealthStatus,
    pub new_health: ServiceHealthStatus,
    pub timestamp: DateTime<Utc>,
}

/// What kind of health transition occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HealthChangeKind {
    /// Recovered from offline/degraded to healthy.
    Recovered,
    /// Degraded from healthy (or unknown) to degraded.
    Degraded,
    /// Failed — any state to offline.
    Failed,
}

impl HealthChangeKind {
    /// Stable name for metrics registration.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Recovered => "recovered",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }

    /// All kind names for `register_domain` with kinds.
    pub const ALL_NAMES: &'static [&'static str] = &["recovered", "degraded", "failed", "probed"];
}

/// Classify a health transition into a change kind, if it is interesting.
///
/// Returns `None` when old == new (no transition).
pub(super) fn classify_transition(
    old: &ServiceHealthStatus,
    new: &ServiceHealthStatus,
) -> Option<HealthChangeKind> {
    if old == new {
        return None;
    }
    Some(match new {
        ServiceHealthStatus::Healthy => HealthChangeKind::Recovered,
        ServiceHealthStatus::Degraded => HealthChangeKind::Degraded,
        ServiceHealthStatus::Offline => HealthChangeKind::Failed,
    })
}
