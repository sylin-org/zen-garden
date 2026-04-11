//! Tool domain events — the internal channel.
//!
//! `ToolChanged` is the richer domain event consumed by projection
//! tasks, metrics, and internal subscribers. It sits alongside the
//! existing wire format `garden_common::tools::ToolDelta`, which
//! continues to carry SSE and UDP beacon traffic consumed by external
//! clients.
//!
//! Every command on the [`Tool`](super::aggregate::Tool) aggregate
//! emits one or more `ToolChanged` events on `Tool::changes()` **and**
//! publishes the corresponding `ToolDelta` to `Tool::delta_stream()`
//! from the same command gateway. Consumers pick the stream that
//! matches their needs.
//!
//! See ARCH-0019 §"`ToolChanged` vs `ToolDelta`" for the pattern
//! deviation justification.

use super::registry::EntryOrigin;
use garden_common::tools::ToolDelta;

/// Internal domain event emitted by every `Tool` command.
///
/// Carries the wire-format `ToolDelta` plus metadata (origin, cursor,
/// batch counts) that never leaves the process — these fields would
/// not survive the SSE / UDP serialization contract and are only
/// consumed by in-process subscribers. The wire format is
/// [`ToolDelta`](garden_common::tools::ToolDelta) on
/// [`Tool::delta_stream`](super::aggregate::Tool::delta_stream).
#[derive(Debug, Clone)]
pub enum ToolChanged {
    /// A registry entry was added or modified. `delta.kind == Upsert`.
    Upserted {
        delta: ToolDelta,
        origin: EntryOrigin,
        cursor: u64,
    },

    /// A registry entry was removed. `delta.kind == Remove`.
    Removed { delta: ToolDelta, cursor: u64 },

    /// Batch TTL reap — N expired gateway entries removed in one pass.
    Reaped { count: usize, cursor: u64 },

    /// Remote beacon applied from a peer stone. `delta_count` covers
    /// the number of entries the beacon affected (upserts + removes).
    BeaconApplied {
        stone_id: String,
        delta_count: usize,
        cursor: u64,
    },

    /// A stone announced goodbye or went offline; all its entries were
    /// removed in one pass.
    StoneRemoved {
        stone_id: String,
        delta_count: usize,
        cursor: u64,
    },
}

impl ToolChanged {
    /// Which [`ChangeKind`] this event belongs to (for metrics).
    pub fn kind(&self) -> ChangeKind {
        match self {
            ToolChanged::Upserted { .. } => ChangeKind::Upserted,
            ToolChanged::Removed { .. } => ChangeKind::Removed,
            ToolChanged::Reaped { .. } => ChangeKind::Reaped,
            ToolChanged::BeaconApplied { .. } => ChangeKind::BeaconApplied,
            ToolChanged::StoneRemoved { .. } => ChangeKind::StoneRemoved,
        }
    }

    /// Extract the wire-format delta, if this event corresponds to a
    /// single delta. Batch events (`Reaped`, `BeaconApplied`,
    /// `StoneRemoved`) return `None`; subscribers that need the
    /// individual deltas should subscribe to
    /// [`Tool::delta_stream`](super::aggregate::Tool::delta_stream)
    /// instead.
    pub fn as_delta(&self) -> Option<&ToolDelta> {
        match self {
            ToolChanged::Upserted { delta, .. } | ToolChanged::Removed { delta, .. } => Some(delta),
            _ => None,
        }
    }

    /// Cursor at which this event was recorded. Monotonically
    /// increasing within a single process lifetime.
    pub fn cursor(&self) -> u64 {
        match self {
            ToolChanged::Upserted { cursor, .. }
            | ToolChanged::Removed { cursor, .. }
            | ToolChanged::Reaped { cursor, .. }
            | ToolChanged::BeaconApplied { cursor, .. }
            | ToolChanged::StoneRemoved { cursor, .. } => *cursor,
        }
    }
}

/// Metric-facing classification of a [`ToolChanged`] event.
///
/// Registered with [`Metrics::register_domain`] at aggregate construction
/// time so the hot path is lock-free per the
/// [register-with-kinds pattern](super::aggregate::Tool::new).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Upserted,
    Removed,
    Reaped,
    BeaconApplied,
    StoneRemoved,
}

impl ChangeKind {
    /// Stable string identifier — the value stored in the Metrics
    /// per-domain event counter map.
    pub fn name(self) -> &'static str {
        match self {
            ChangeKind::Upserted => "upserted",
            ChangeKind::Removed => "removed",
            ChangeKind::Reaped => "reaped",
            ChangeKind::BeaconApplied => "beacon-applied",
            ChangeKind::StoneRemoved => "stone-removed",
        }
    }

    /// All change kinds, for `Metrics::register_domain`.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "upserted",
        "removed",
        "reaped",
        "beacon-applied",
        "stone-removed",
    ];
}
