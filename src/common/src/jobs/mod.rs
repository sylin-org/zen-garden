//! Job pipeline for background tasks
//!
//! Supports async job execution with:
//! - Job queuing and persistence (JSON files)
//! - Progress tracking via events
//! - Executor pattern for different job types
//! - Restart capability on daemon crash

pub mod manager;
pub mod persistence;
pub mod retry;
pub mod types;

pub use manager::Jobs;
pub use persistence::JsonJobPersistence;
pub use retry::{retry_simple, retry_with_policy, RetryPolicy};
pub use types::{Job, JobId, JobInput, JobOutput, JobStatus};
