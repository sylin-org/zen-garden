//! `Jobs` aggregate — DDD root of the Jobs bounded context.
//!
//! Ch3 of ARCH-0021 (Book IV of ARCH-0017). Wraps a shared
//! `Arc<RwLock<JobsState>>` with a typed command/query surface, an
//! `Arc<Metrics>` injection, a `JobsChanged` internal event stream,
//! and parallel wire-format emission of `JobEvent` through the
//! existing `EventBus` (dual event streams — see ARCH-0019 Book II
//! precedent and `docs/specs/domain-aggregates.md`).
//!
//! ## Ownership
//!
//! During the Book IV strangler phase (Ch3 → Ch5), the aggregate's
//! inner state is an `Arc` shared with the legacy
//! `Moss::jobs: Arc<RwLock<HashMap<String, Job>>>` field. Both
//! views see the same `HashMap`. Mutations through the legacy raw-map
//! path do **not** fire aggregate events — that is the whole point
//! of the strangler: call sites migrate file by file, each migrated
//! site gains event emission automatically. Ch5 deletes the legacy
//! field once every caller has been migrated.
//!
//! ## Infallible mutations
//!
//! `Jobs` commands are **infallible** — all mutations return `()` (or
//! a value) and no `JobsError` type exists. A mutation addressed at a
//! missing job id is treated as a warn-level no-op. This matches the
//! Book I (`Metrics`) deviation: an ephemeral aggregate whose
//! mutations cannot fail in a domain-meaningful way (no persistence
//! to flunk, no invariants for the command to violate, no external
//! port to propagate errors from). The decision is recorded as a
//! minor pattern note in ARCH-0021 §Pattern deviations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::{RwLock, broadcast};

use super::event::{ChangeKind, EvictionReason, JobsChanged};
use super::maintenance::{DEFAULT_TERMINAL_TTL, ReapReport, is_expired};
use super::state::JobsState;
use super::{Job, JobStatus};

use crate::domain::Metrics;
use crate::domain::events::JobEvent;
use crate::infra::EventBus;

/// Capacity of the internal `JobsChanged` broadcast channel.
///
/// Large enough to absorb the burst that a batch-install of ~20
/// services emits across `submit → start → record_item_* → complete`
/// without lagging the reaper task subscriber.
const CHANGES_CHANNEL_CAPACITY: usize = 512;

/// `Jobs` bounded context.
///
/// Ephemeral aggregate — no persistence, no `JobStore` port. State is
/// rebuilt from empty on every process start. The legacy-strangler
/// field `state` is `pub(crate)` during Ch3 because Ch4/Ch5 migrate
/// call sites incrementally; once the last raw-map site is gone the
/// field flips back to `pub(super)`.
#[derive(Clone)]
pub struct Jobs {
    /// Shared active-jobs map.
    ///
    /// During the strangler phase, the same `Arc` is held by
    /// `Moss::jobs`. Both views read/write the same `HashMap`.
    pub(crate) state: Arc<RwLock<JobsState>>,

    /// Metrics aggregate for latency + per-kind counters.
    metrics: Arc<Metrics>,

    /// Internal `JobsChanged` broadcast — subscribed by the reaper
    /// task, the pre-install completion watcher, and future Book IV+
    /// projections.
    changes: broadcast::Sender<JobsChanged>,

    /// Wire-format event bus for the parallel `JobEvent` stream that
    /// rake / dashboard SSE clients consume today.
    event_bus: EventBus,
}

impl Jobs {
    /// Registered domain name for Metrics.
    pub const NAME: &'static str = "jobs";

    /// Construct a new `Jobs` aggregate sharing the given state `Arc`.
    ///
    /// The caller (`bootstrap::run`) creates the `Arc<RwLock<...>>`
    /// once, passes a clone to this constructor, and stores a second
    /// clone in `Moss::jobs` as the legacy raw-map field. Both
    /// views see the same `HashMap` throughout Ch3–Ch5.
    pub async fn with_shared_state(
        state: Arc<RwLock<JobsState>>,
        metrics: Arc<Metrics>,
        event_bus: EventBus,
    ) -> Self {
        metrics
            .register_domain(Self::NAME, ChangeKind::ALL_NAMES)
            .await;
        let (changes, _) = broadcast::channel(CHANGES_CHANNEL_CAPACITY);
        Self {
            state,
            metrics,
            changes,
            event_bus,
        }
    }

    // ── Commands ────────────────────────────────────────────────────────

    /// Submit a new job. Inserts the job as `Pending` and emits
    /// `JobsChanged::Submitted`. No wire `JobEvent` fires on submit —
    /// the public wire contract treats "job was started" as the first
    /// observable transition; submit is internal-only.
    ///
    /// `targets` is what the legacy `Job.offerings` field stores — a
    /// list of service names (for install jobs) or capability names
    /// (for capability-refresh / add jobs). The semantic overload is
    /// tracked in `docs/scaffolding.md` under the
    /// `deferred-job-offerings-field` entry for post-epic API
    /// realignment.
    #[tracing::instrument(level = "debug", skip(self, targets), fields(jobs.id = %id, jobs.operation = %operation))]
    pub async fn submit(&self, id: String, operation: &str, targets: Vec<String>) -> Job {
        let started = Instant::now();
        let target_count = targets.len();
        let job = Job {
            id: id.clone(),
            offerings: targets,
            status: JobStatus::Pending,
            completed: Vec::new(),
            failed: HashMap::new(),
            started_at: SystemTime::now(),
            completed_at: None,
        };
        self.state.write().await.insert(id.clone(), job.clone());
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.emit(JobsChanged::Submitted {
            id,
            operation: operation.to_string(),
            target_count,
        })
        .await;
        job
    }

    /// Move a job from `Pending` to `Running`. Emits
    /// `JobsChanged::Started` + wire `JobEvent::Started`. No-op with
    /// a warn-level trace if the id is unknown.
    #[tracing::instrument(level = "debug", skip(self), fields(jobs.id = %id, jobs.operation = %operation, jobs.offering = %offering))]
    pub async fn start(&self, id: &str, operation: &str, offering: &str) {
        let started = Instant::now();
        let mutated = {
            let mut guard = self.state.write().await;
            if let Some(job) = guard.get_mut(id) {
                job.status = JobStatus::Running;
                true
            } else {
                false
            }
        };
        if !mutated {
            tracing::warn!(jobs.id = id, "Jobs::start called on unknown job id");
            return;
        }
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.event_bus
            .emit(JobEvent::started(id, offering, operation));
        self.emit(JobsChanged::Started {
            id: id.to_string(),
            offering: offering.to_string(),
        })
        .await;
    }

    /// Append `item` to the job's `completed` list. No-op with a
    /// warn-level trace if the id is unknown.
    #[tracing::instrument(level = "trace", skip(self), fields(jobs.id = %id, jobs.item = %item))]
    pub async fn record_item_completed(&self, id: &str, item: String) {
        let started = Instant::now();
        let completed_total = {
            let mut guard = self.state.write().await;
            if let Some(job) = guard.get_mut(id) {
                job.completed.push(item.clone());
                job.completed.len()
            } else {
                tracing::warn!(
                    jobs.id = id,
                    "Jobs::record_item_completed called on unknown job id"
                );
                return;
            }
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.emit(JobsChanged::ItemCompleted {
            id: id.to_string(),
            item,
            completed_total,
        })
        .await;
    }

    /// Insert `(item, error)` into the job's `failed` map. Emits
    /// `JobsChanged::ItemFailed` + wire `JobEvent::Failed` (the wire
    /// event uses the item as the "offering" label so rake's existing
    /// SSE consumers keep rendering per-item failures). No-op on
    /// unknown id.
    #[tracing::instrument(level = "debug", skip(self), fields(jobs.id = %id, jobs.item = %item))]
    pub async fn record_item_failed(&self, id: &str, item: String, error: String) {
        let started = Instant::now();
        let failed_total = {
            let mut guard = self.state.write().await;
            if let Some(job) = guard.get_mut(id) {
                job.failed.insert(item.clone(), error.clone());
                job.failed.len()
            } else {
                tracing::warn!(
                    jobs.id = id,
                    "Jobs::record_item_failed called on unknown job id"
                );
                return;
            }
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.event_bus.emit(JobEvent::failed(id, &item, &error));
        self.emit(JobsChanged::ItemFailed {
            id: id.to_string(),
            item,
            error,
            failed_total,
        })
        .await;
    }

    /// Finalize a job as `Completed`. Sets `completed_at = now`, emits
    /// `JobsChanged::Completed` + wire `JobEvent::Completed`. No-op on
    /// unknown id.
    #[tracing::instrument(level = "debug", skip(self), fields(jobs.id = %id, jobs.offering = %offering))]
    pub async fn complete(&self, id: &str, offering: &str) {
        let started = Instant::now();
        let duration_ms = {
            let mut guard = self.state.write().await;
            if let Some(job) = guard.get_mut(id) {
                job.status = JobStatus::Completed;
                let completed_at = SystemTime::now();
                job.completed_at = Some(completed_at);
                completed_at
                    .duration_since(job.started_at)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            } else {
                tracing::warn!(jobs.id = id, "Jobs::complete called on unknown job id");
                return;
            }
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.event_bus.emit(JobEvent::completed(id, offering));
        self.emit(JobsChanged::Completed {
            id: id.to_string(),
            offering: offering.to_string(),
            duration_ms,
        })
        .await;
    }

    /// Finalize a job as `Failed`. Optionally records a trailing
    /// `(key, error)` pair in the `failed` map before finalizing (the
    /// combined "insert + fail" pattern the executors use today for
    /// single-item jobs). Sets `completed_at = now`, emits
    /// `JobsChanged::Failed` + wire `JobEvent::Failed`. No-op on
    /// unknown id.
    #[tracing::instrument(level = "debug", skip(self, last_error), fields(jobs.id = %id, jobs.offering = %offering))]
    pub async fn fail(&self, id: &str, offering: &str, last_error: Option<(String, String)>) {
        let started = Instant::now();
        let outcome = {
            let mut guard = self.state.write().await;
            let Some(job) = guard.get_mut(id) else {
                tracing::warn!(jobs.id = id, "Jobs::fail called on unknown job id");
                return;
            };
            if let Some((key, error)) = last_error.as_ref() {
                job.failed.insert(key.clone(), error.clone());
            }
            job.status = JobStatus::Failed;
            let completed_at = SystemTime::now();
            job.completed_at = Some(completed_at);
            let duration_ms = completed_at
                .duration_since(job.started_at)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            (duration_ms, job.failed.len())
        };
        let (duration_ms, failure_count) = outcome;
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        let wire_error = last_error
            .as_ref()
            .map(|(_, e)| e.as_str())
            .unwrap_or("job failed");
        self.event_bus
            .emit(JobEvent::failed(id, offering, wire_error));
        self.emit(JobsChanged::Failed {
            id: id.to_string(),
            offering: offering.to_string(),
            duration_ms,
            failure_count,
        })
        .await;
    }

    /// Sweep terminal jobs past the default TTL. Called by the
    /// `JobsReaperTask` background task (wired in Ch5).
    pub async fn maintain(&self) -> ReapReport {
        self.maintain_with(SystemTime::now(), DEFAULT_TERMINAL_TTL)
            .await
    }

    /// Sweep with explicit `now` + `ttl`, for tests and for future
    /// admin-triggered manual sweeps.
    pub async fn maintain_with(&self, now: SystemTime, ttl: Duration) -> ReapReport {
        let started = Instant::now();
        let evicted_ids: Vec<String> = {
            let mut guard = self.state.write().await;
            let expired: Vec<String> = guard
                .iter()
                .filter_map(|(id, job)| {
                    let terminal = matches!(job.status, JobStatus::Completed | JobStatus::Failed);
                    if !terminal {
                        return None;
                    }
                    let completed_at = job.completed_at?;
                    if is_expired(completed_at, now, ttl) {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for id in &expired {
                guard.remove(id);
            }
            expired
        };
        let kept = self.state.read().await.len();
        for id in &evicted_ids {
            self.emit(JobsChanged::Evicted {
                id: id.clone(),
                reason: EvictionReason::TtlExpired,
            })
            .await;
        }
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        ReapReport {
            evicted: evicted_ids.len(),
            kept,
        }
    }

    // ── Queries ────────────────────────────────────────────────────────

    /// Clone the job with the given id, or `None` if unknown.
    pub async fn get(&self, id: &str) -> Option<Job> {
        self.state.read().await.get(id).cloned()
    }

    /// Clone every job currently tracked.
    pub async fn snapshot(&self) -> Vec<Job> {
        self.state.read().await.values().cloned().collect()
    }

    /// Clone every non-terminal job (`Pending` or `Running`).
    pub async fn list_active(&self) -> Vec<Job> {
        self.state
            .read()
            .await
            .values()
            .filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running))
            .cloned()
            .collect()
    }

    /// Count of non-terminal jobs.
    pub async fn active_count(&self) -> usize {
        self.state
            .read()
            .await
            .values()
            .filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running))
            .count()
    }

    /// Find an active (Pending or Running) job whose id starts with
    /// `prefix`. Used by the capability endpoints to detect
    /// duplicate-add / duplicate-refresh requests and return
    /// `InProgress` responses instead of creating a second job.
    /// Returns the first match; order is `HashMap` iteration order
    /// (arbitrary) — duplicate-prefix collisions are caller's problem
    /// to avoid via unique id construction.
    pub async fn find_active_by_prefix(&self, prefix: &str) -> Option<Job> {
        self.state
            .read()
            .await
            .iter()
            .find(|(id, job)| {
                id.starts_with(prefix)
                    && matches!(job.status, JobStatus::Pending | JobStatus::Running)
            })
            .map(|(_, job)| job.clone())
    }

    // ── Events ─────────────────────────────────────────────────────────

    /// Subscribe to the internal `JobsChanged` stream.
    pub fn changes(&self) -> broadcast::Receiver<JobsChanged> {
        self.changes.subscribe()
    }

    // ── Internals ──────────────────────────────────────────────────────

    /// Record the domain event counter and broadcast the event.
    /// Mirrors `Topology::emit` (ARCH-0020) — same shape.
    async fn emit(&self, event: JobsChanged) {
        self.metrics
            .record_domain_event(Self::NAME, event.kind().name())
            .await;
        let _ = self.changes.send(event);
    }
}
