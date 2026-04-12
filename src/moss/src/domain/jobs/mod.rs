//! Jobs bounded context — Book IV of [ARCH-0017].
//!
//! **Chapter 2 state (this commit):** only the value objects live here.
//! `Job` and `JobStatus` have moved out of `crate::app_state` into
//! [`entry`]. The aggregate skeleton (`Jobs`, commands, queries,
//! `JobsChanged` event, `Metrics` injection, reaper) lands in
//! Chapter 3.
//!
//! The `AppState::jobs: Arc<RwLock<HashMap<String, Job>>>` field is
//! unchanged during Ch2. Every call site still reaches into the raw
//! map — migration happens in Ch4 (executors) and Ch5 (API handlers,
//! bootstrap, service lifecycle).
//!
//! [ARCH-0017]: ../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

pub mod entry;

pub use entry::{Job, JobStatus};
