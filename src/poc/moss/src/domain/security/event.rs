//! Security domain events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Security domain event — emitted by the Security aggregate on mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityChanged {
    pub kind: SecurityChangeKind,
    pub timestamp: DateTime<Utc>,
}

/// What changed in the Security domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityChangeKind {
    /// Stone enrolled in a pond (placed keystone or joined).
    Enrolled { cornerstone: Option<String> },
    /// Stone unenrolled from a pond (pond drained or cert revoked).
    Unenrolled,
    /// Pond decorative name changed.
    PondRenamed { name: String },
}

impl SecurityChangeKind {
    /// Stable name for Metrics per-kind counter lookup.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Enrolled { .. } => "enrolled",
            Self::Unenrolled => "unenrolled",
            Self::PondRenamed { .. } => "pond_renamed",
        }
    }

    /// All variant names for Metrics registration.
    pub const ALL_NAMES: &'static [&'static str] = &["enrolled", "unenrolled", "pond_renamed"];
}
