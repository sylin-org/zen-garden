//! Job entry types — the domain value objects that back the Jobs aggregate.
//!
//! These types previously lived inline in `crate::app_state` alongside the
//! `Moss` struct, violating code-standards §14 (one concept per file).
//! Book IV Chapter 2 of [ARCH-0017] moved them here without semantic change:
//! same fields, same derives, same serde representation, same public API.
//!
//! The aggregate that owns mutation of these values lives in
//! [`super::aggregate`] and is introduced in Chapter 3. For the duration of
//! Ch2, the old `Moss::jobs: Arc<RwLock<HashMap<String, Job>>>` field
//! continues to hold these values directly — only the type *definitions*
//! have moved.
//!
//! [ARCH-0017]: ../../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

use std::collections::HashMap;
use std::time::SystemTime;

/// Lifecycle state of a background job.
///
/// `Pending` is transient today — every executor inserts a job as
/// `Pending` and immediately promotes it to `Running`. The variant is
/// preserved because it is serialized on the wire (`/api/v1/jobs`
/// responses) and matched in duplicate-job detection
/// (`crate::api::v1::offering_capabilities`). A future queueing layer
/// may bring it back to life without schema churn.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Background job for tracking long-running operations.
///
/// `offerings` is semantically overloaded today: install jobs store
/// service names, capability-refresh/capability-add jobs repurpose it
/// to hold capability names so progress tracking can drive the
/// `completed.len() / offerings.len()` ratio. A rename to `targets`
/// is a breaking wire-format change tracked in
/// `docs/scaffolding.md` under the `deferred-job-offerings-field`
/// entry for the post-epic API realignment project.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Job {
    pub id: String,
    pub offerings: Vec<String>,
    pub status: JobStatus,
    pub completed: Vec<String>,
    pub failed: HashMap<String, String>, // key -> error message
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
}
