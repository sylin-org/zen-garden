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
/// Two progress models live on the same struct:
///
/// 1. **Batch jobs** (install, refresh-capabilities) — track items in
///    `targets`/`completed`/`failed`. Progress is
///    `completed.len() / targets.len()`.
/// 2. **Single-operation jobs** (capture_snapshot, plant_snapshot) —
///    one target, but multi-step internal flow. `current_step` /
///    `total_steps` / `last_message` describe per-step progress; the
///    final value lands in `result` on completion.
///
/// `targets`/`completed`/`failed` and `current_step`/`total_steps` are
/// orthogonal — single-op jobs use the latter, batch jobs use the
/// former. Both surfaces always serialise; consumers ignore what
/// doesn't apply to their job kind.
///
/// The Rust field is `targets`; the wire name stays `"offerings"` via
/// `#[serde(rename)]` for backward compatibility with rake and
/// dashboard consumers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Job {
    pub id: String,
    /// Operation name — e.g. `"install"`, `"capture_snapshot"`,
    /// `"plant_snapshot"`. Empty for legacy jobs created before
    /// operation tracking landed; defaults via `#[serde(default)]`
    /// for forward-compatible deserialisation.
    #[serde(default)]
    pub operation: String,
    #[serde(rename = "offerings")]
    pub targets: Vec<String>,
    pub status: JobStatus,
    pub completed: Vec<String>,
    pub failed: HashMap<String, String>, // key -> error message
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,

    /// Current step (1-indexed) for single-operation progress. `None`
    /// for batch jobs and pre-step jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_step: Option<u32>,

    /// Total expected steps. Set by the operation once the count is
    /// known (e.g. capture computes it after listing volumes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<u32>,

    /// Most recent human-readable progress message — drives the
    /// label/tooltip on Pavilion's seed-chip and similar surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,

    /// Result payload set on `Completed`. Opaque JSON whose shape
    /// depends on `operation` — capture returns `CapturedSnapshot`,
    /// plant returns `PlantedSnapshot`, etc. Consumers dispatch on
    /// `operation` to decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Top-level error message set on `Failed`. Single-op jobs surface
    /// the operation-level failure here; batch jobs may leave this
    /// empty and use `failed[target]` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
