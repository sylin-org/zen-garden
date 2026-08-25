//! `JobsState` — the mutable state owned by the `Jobs` aggregate.
//!
//! A thin alias over `HashMap<String, Job>`. During the Book IV
//! strangler phase (Ch3 → Ch5), the same `Arc<RwLock<JobsState>>` is
//! shared with the legacy `Moss::jobs` field so existing raw-map
//! call sites keep working while migrations land file by file.
//! Ch5 deletes the legacy field and flips `Moss::jobs` to
//! `Arc<Jobs>` exclusively.

use std::collections::HashMap;

use super::Job;

/// Active + recently-terminal jobs, keyed by job id.
///
/// Terminal entries (Completed / Failed) are evicted by
/// [`super::Jobs::maintain`] once their age exceeds the terminal TTL
/// in [`super::maintenance::DEFAULT_TERMINAL_TTL`].
pub(super) type JobsState = HashMap<String, Job>;
