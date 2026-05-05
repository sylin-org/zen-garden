//! `GardenEvent` — Pavilion's internal event vocabulary.
//!
//! Observers translate protocol-level signals (SSE frames, awareness
//! transitions) into these records. The Announcer policy layer then
//! decides which ones land in the activity log only and which also
//! promote to a toast.
//!
//! The enum is intentionally narrower than what Moss exposes: an
//! activity entry needs a stable shape for the UI, not a faithful
//! mirror of the wire format.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Severity drives the dot color in Activity rows and the toast
/// scenario when the event is promoted (info / warning / urgent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — background activity, low signal.
    Info,
    /// Notable — something the user might want to know.
    Notice,
    /// Action-worthy — something likely needs attention.
    Warn,
    /// Critical — sync failed, stone offline mid-operation.
    /// Reserved for events the policy hasn't yet emitted; kept here
    /// so the wire enum is stable from day one.
    #[allow(dead_code)]
    Urgent,
}

/// Narrow event vocabulary for the Activity view and toast pipeline.
///
/// Each variant carries the minimum fields the UI needs to render a
/// row and the policy layer needs to dedupe / coalesce. Additional
/// detail (cursor IDs, replica counts) belongs upstream in the
/// observer or downstream in the user's pull-on-demand fetches.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GardenEvent {
    /// A stone became visible in awareness for the first time.
    StoneJoined {
        stone_id: String,
        stone_name: String,
        endpoint: String,
    },
    /// A stone was evicted from awareness (TTL expired).
    StoneLeft {
        stone_id: String,
        stone_name: String,
    },
    /// Storage changelog advanced on the tended stone — a coalesced
    /// summary across some window. Volume figures are post-coalesce
    /// totals for the window the policy chose.
    StorageActivity {
        stone_name: String,
        bank_name: String,
        creates: u32,
        modifies: u32,
        deletes: u32,
    },
}

impl GardenEvent {
    /// Coalescing key — events with the same key inside the policy
    /// window collapse into one entry.
    pub fn dedupe_key(&self) -> String {
        match self {
            GardenEvent::StoneJoined { stone_id, .. } => format!("stone-joined:{stone_id}"),
            GardenEvent::StoneLeft { stone_id, .. } => format!("stone-left:{stone_id}"),
            GardenEvent::StorageActivity {
                stone_name,
                bank_name,
                ..
            } => format!("storage-activity:{stone_name}:{bank_name}"),
        }
    }

    /// Severity policy. Stable across observer/policy boundaries so
    /// the UI can colour rows consistently.
    pub fn severity(&self) -> Severity {
        match self {
            GardenEvent::StoneJoined { .. } => Severity::Notice,
            GardenEvent::StoneLeft { .. } => Severity::Warn,
            GardenEvent::StorageActivity { .. } => Severity::Info,
        }
    }
}

/// One row in the Activity ring buffer. Carries the event plus the
/// time it was accepted by the Announcer, plus a `promoted` flag
/// telling the UI whether the user *also* saw a toast for this.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityEntry {
    pub id: String,
    pub at: DateTime<Utc>,
    pub event: GardenEvent,
    pub severity: Severity,
    pub promoted: bool,
}
