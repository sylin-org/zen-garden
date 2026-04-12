//! TTL policy and sweep report for the `Jobs` reaper.
//!
//! Active jobs (`Pending`, `Running`) are **never** evicted — a stuck
//! job is a bug worth surfacing, not a memory leak to hide. Only
//! terminal jobs (`Completed`, `Failed`) are eligible for eviction,
//! and only once they are older than [`DEFAULT_TERMINAL_TTL`].
//!
//! The reaper itself lives in `tasks::jobs_reaper` (Ch5); this module
//! owns only the policy constants and the pure helpers the aggregate
//! uses inside its write guard.

use std::time::{Duration, SystemTime};

/// Default retention window for terminal jobs.
///
/// Terminal jobs whose `completed_at` is older than this are swept
/// by [`super::Jobs::maintain`]. 24 hours balances "long enough for an
/// operator to still read a failure post-mortem" against "short enough
/// to keep the in-memory map from drifting unbounded on a stone that
/// completes hundreds of jobs per day".
pub const DEFAULT_TERMINAL_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Summary of a single `maintain` sweep.
///
/// `evicted` is the number of terminal jobs removed from the map on
/// this sweep. `kept` is the post-sweep map size (active + still-
/// in-TTL terminal jobs). A sweep that evicts zero jobs is silent —
/// no events fire, only the mutation latency is recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReapReport {
    pub evicted: usize,
    pub kept: usize,
}

impl ReapReport {
    pub fn is_empty(&self) -> bool {
        self.evicted == 0
    }
}

/// Whether `completed_at` is older than `ttl` relative to `now`.
///
/// Clock skew (a `completed_at` in the future) is treated as
/// "not expired" — the job is kept. This matches the defensive
/// posture the rest of moss takes toward clock weirdness.
pub(super) fn is_expired(completed_at: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    match now.duration_since(completed_at) {
        Ok(age) => age >= ttl,
        Err(_) => false,
    }
}
