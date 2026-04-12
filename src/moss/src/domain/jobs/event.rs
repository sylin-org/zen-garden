//! `JobsChanged` — the internal domain event stream emitted by the
//! `Jobs` aggregate on every mutation.
//!
//! This stream carries the rich internal representation (`id`,
//! `operation`, per-event metadata) and is consumed by process-local
//! subscribers such as the reaper task (Ch5), the bootstrap
//! pre-install completion watcher (Ch5), and future book projections.
//!
//! The pre-existing **wire-format** stream `JobEvent`
//! (`crate::domain::events::JobEvent`) continues to flow through
//! `EventBus` unchanged — public SSE clients (rake, dashboards) keep
//! consuming it. Every aggregate command that maps to a `JobEvent`
//! variant emits *both* streams atomically. This is the Book II
//! "Dual event streams" pattern deviation documented in
//! `docs/specs/domain-aggregates.md`.

use serde::Serialize;

/// Reason a job was evicted from the active set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionReason {
    /// The job finished (Completed or Failed) more than the terminal
    /// TTL ago and was swept by [`super::Jobs::maintain`].
    TtlExpired,
}

/// Internal domain event emitted on every mutation of the `Jobs`
/// aggregate's state.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum JobsChanged {
    /// New job inserted via `submit`. Status is `Pending`.
    Submitted {
        id: String,
        operation: String,
        target_count: usize,
    },
    /// Job transitioned `Pending → Running`.
    Started { id: String, offering: String },
    /// An item under a job was marked successful.
    ItemCompleted {
        id: String,
        item: String,
        completed_total: usize,
    },
    /// An item under a job was marked failed.
    ItemFailed {
        id: String,
        item: String,
        error: String,
        failed_total: usize,
    },
    /// Job reached terminal `Completed` state.
    Completed {
        id: String,
        offering: String,
        duration_ms: u64,
    },
    /// Job reached terminal `Failed` state.
    Failed {
        id: String,
        offering: String,
        duration_ms: u64,
        failure_count: usize,
    },
    /// Job was evicted from the active set by the reaper.
    Evicted { id: String, reason: EvictionReason },
}

/// Metric kind for `Metrics::record_domain_event` — one variant per
/// `JobsChanged` shape. Registered with the Metrics aggregate at
/// construction via [`ChangeKind::ALL_NAMES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Submitted,
    Started,
    ItemCompleted,
    ItemFailed,
    Completed,
    Failed,
    Evicted,
}

impl ChangeKind {
    /// Static list of all kind names — passed to
    /// `Metrics::register_domain` so the per-kind counters are
    /// pre-populated and the hot-path reads are lock-free.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "submitted",
        "started",
        "item_completed",
        "item_failed",
        "completed",
        "failed",
        "evicted",
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Started => "started",
            Self::ItemCompleted => "item_completed",
            Self::ItemFailed => "item_failed",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Evicted => "evicted",
        }
    }
}

impl JobsChanged {
    pub fn kind(&self) -> ChangeKind {
        match self {
            Self::Submitted { .. } => ChangeKind::Submitted,
            Self::Started { .. } => ChangeKind::Started,
            Self::ItemCompleted { .. } => ChangeKind::ItemCompleted,
            Self::ItemFailed { .. } => ChangeKind::ItemFailed,
            Self::Completed { .. } => ChangeKind::Completed,
            Self::Failed { .. } => ChangeKind::Failed,
            Self::Evicted { .. } => ChangeKind::Evicted,
        }
    }

    /// The job id every event carries.
    pub fn id(&self) -> &str {
        match self {
            Self::Submitted { id, .. }
            | Self::Started { id, .. }
            | Self::ItemCompleted { id, .. }
            | Self::ItemFailed { id, .. }
            | Self::Completed { id, .. }
            | Self::Failed { id, .. }
            | Self::Evicted { id, .. } => id,
        }
    }
}
