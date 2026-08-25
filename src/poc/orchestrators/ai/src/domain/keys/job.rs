//! Job reference fields — populated on Async and Streaming outcomes.

use crate::domain::field_path::FieldPath;

pub const ID: FieldPath = FieldPath::new("job.id");
pub const STATUS: FieldPath = FieldPath::new("job.status");
pub const ETA_SECONDS: FieldPath = FieldPath::new("job.eta_seconds");
pub const PROGRESS_CURRENT: FieldPath = FieldPath::new("job.progress.current");
pub const PROGRESS_TOTAL: FieldPath = FieldPath::new("job.progress.total");
pub const PROGRESS_LABEL: FieldPath = FieldPath::new("job.progress.label");

pub mod values {
    pub const STATUS_QUEUED: &str = "queued";
    pub const STATUS_RUNNING: &str = "running";
    pub const STATUS_DONE: &str = "done";
    pub const STATUS_FAILED: &str = "failed";
    pub const STATUS_CANCELLED: &str = "cancelled";
}
