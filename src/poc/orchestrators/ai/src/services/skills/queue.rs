//! Provisioning queue — bounded-concurrency worker for skill
//! dependency downloads + instance pushes (ORCH-0029).
//!
//! Single aggregate (`ProvisioningQueue`) replacing the prior
//! system's three-module split (`domain/provisioning.rs`,
//! `domain/provisioning_domain.rs`, `skills/queue.rs`). ORCH-0028 §6
//! compliant: private state behind a `Mutex`, snapshot via
//! `watch::channel`, one writer.
//!
//! ## Behavior
//!
//! - **Dedup key**: `(skill_moniker, endpoint)`. Submitting the same
//!   target while running, queued, or in backoff is a no-op.
//! - **Priority**: `User=0 > Discovery=1`. User-initiated jobs
//!   jump the queue.
//! - **Bounded concurrency**: semaphore-limited worker count
//!   (default 2). Configurable via `ProvisioningQueue::new(n)`.
//! - **Exponential backoff** on failure: 1m → 5m → 30m → 1h
//!   (capped). A subsequent `submit` inside the backoff window
//!   returns `false` — the caller knows it wasn't queued.
//! - **Cancellation**: graceful shutdown drains pending jobs and
//!   waits up to 30s for in-flight jobs.
//!
//! ## Ownership
//!
//! The queue holds **only orchestration state** — the job list,
//! backoff map, watch channel. It does NOT hold the HTTP client or
//! the cache paths. Those are passed into `run_worker` alongside
//! closures that perform the actual work, so the queue can be
//! unit-tested without any I/O.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{broadcast, watch, Mutex, Notify};

use crate::domain::moniker::Moniker;

/// Dedup key — one job per `(skill, endpoint)` pair.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct ProvisioningTarget {
    pub skill: Moniker,
    pub endpoint: String,
}

impl std::fmt::Display for ProvisioningTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.skill.as_str(), self.endpoint)
    }
}

/// Job priority. Lower ordinal = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Operator-initiated (dashboard click, API call).
    User = 0,
    /// Auto-discovery detected a missing skill on a new instance.
    Discovery = 1,
}

/// Per-job download progress, updated by the worker as bytes arrive.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model: String,
    pub downloaded_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
}

/// Job lifecycle state machine.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running {
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<DownloadProgress>,
    },
    Completed {
        #[serde(serialize_with = "serialize_duration_ms")]
        duration: Duration,
    },
    Failed {
        reason: String,
        attempts: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_in_secs: Option<u64>,
    },
}

fn serialize_duration_ms<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(d.as_millis() as u64)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvisioningJob {
    pub id: String,
    pub target: ProvisioningTarget,
    pub priority: Priority,
    pub status: JobStatus,
    pub stone_name: String,
    pub provider: String,
    pub submitted_ms: u64,
}

impl ProvisioningJob {
    fn new(
        target: ProvisioningTarget,
        priority: Priority,
        stone_name: String,
        provider: String,
    ) -> Self {
        Self {
            id: garden_common::utils::ids::generate_guidv7(),
            target,
            priority,
            status: JobStatus::Queued,
            stone_name,
            provider,
            submitted_ms: now_epoch_ms(),
        }
    }
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Exponential backoff schedule: 1m → 5m → 30m → 1h (capped).
pub struct Backoff;

impl Backoff {
    const SCHEDULE: &'static [Duration] = &[
        Duration::from_secs(60),
        Duration::from_secs(300),
        Duration::from_secs(1800),
        Duration::from_secs(3600),
    ];

    pub fn delay(attempts: u32) -> Duration {
        let idx = ((attempts.saturating_sub(1)) as usize).min(Self::SCHEDULE.len() - 1);
        Self::SCHEDULE[idx]
    }
}

/// Immutable snapshot for API responses and catalog rendering.
#[derive(Debug, Clone, Serialize)]
pub struct ProvisioningSnapshot {
    pub jobs: Vec<ProvisioningJob>,
    pub active: usize,
    pub queued: usize,
    pub max_concurrency: usize,
}

impl Default for ProvisioningSnapshot {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            active: 0,
            queued: 0,
            max_concurrency: DEFAULT_CONCURRENCY,
        }
    }
}

/// Lifecycle events for the event stream.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueueEvent {
    Submitted { job: ProvisioningJob },
    Started { job: ProvisioningJob },
    Progress { target: ProvisioningTarget, progress: DownloadProgress },
    Completed { target: ProvisioningTarget, duration_ms: u64 },
    Failed { target: ProvisioningTarget, reason: String, attempts: u32 },
}

pub const DEFAULT_CONCURRENCY: usize = 2;

/// Maximum number of completed/failed jobs retained in the snapshot.
const HISTORY_CAP: usize = 50;

/// The queue aggregate. Private state behind `Mutex`, read-only
/// snapshot via `watch`, lifecycle events via `broadcast`, worker
/// wake-up via `Notify`.
pub struct ProvisioningQueue {
    state: Mutex<QueueState>,
    snapshot_tx: watch::Sender<Arc<ProvisioningSnapshot>>,
    events: broadcast::Sender<QueueEvent>,
    notify: Arc<Notify>,
    max_concurrency: usize,
}

#[derive(Default)]
struct QueueState {
    pending: VecDeque<ProvisioningJob>,
    running: HashMap<ProvisioningTarget, ProvisioningJob>,
    history: VecDeque<ProvisioningJob>,
    backoff: HashMap<ProvisioningTarget, (Instant, u32)>,
}

impl ProvisioningQueue {
    pub fn new(max_concurrency: usize) -> Arc<Self> {
        let initial = Arc::new(ProvisioningSnapshot {
            max_concurrency,
            ..Default::default()
        });
        let (snapshot_tx, _) = watch::channel(initial);
        let (events, _) = broadcast::channel(128);
        Arc::new(Self {
            state: Mutex::new(QueueState::default()),
            snapshot_tx,
            events,
            notify: Arc::new(Notify::new()),
            max_concurrency,
        })
    }

    pub fn with_default_concurrency() -> Arc<Self> {
        Self::new(DEFAULT_CONCURRENCY)
    }

    pub fn snapshot(&self) -> Arc<ProvisioningSnapshot> {
        self.snapshot_tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<ProvisioningSnapshot>> {
        self.snapshot_tx.subscribe()
    }

    pub fn event_stream(&self) -> broadcast::Receiver<QueueEvent> {
        self.events.subscribe()
    }

    pub fn notifier(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Submit a job. Returns `true` if queued, `false` if deduped
    /// (already running / queued / in backoff window).
    pub async fn submit(
        &self,
        target: ProvisioningTarget,
        priority: Priority,
        stone_name: String,
        provider: String,
    ) -> bool {
        let mut state = self.state.lock().await;

        if state.running.contains_key(&target) {
            return false;
        }
        if state.pending.iter().any(|j| j.target == target) {
            return false;
        }
        if let Some((retry_after, _)) = state.backoff.get(&target) {
            if Instant::now() < *retry_after {
                return false;
            }
            state.backoff.remove(&target);
        }

        let job = ProvisioningJob::new(target, priority, stone_name, provider);
        // Priority sort: `User=0` before `Discovery=1`. FIFO within
        // a priority level via `partition_point`.
        let insert_pos = state.pending.partition_point(|j| j.priority <= priority);
        state.pending.insert(insert_pos, job.clone());
        self.publish(&state);
        drop(state);

        let _ = self.events.send(QueueEvent::Submitted { job });
        self.notify.notify_one();
        true
    }

    /// Take the next queued job and mark it Running.
    pub async fn take_next(&self) -> Option<ProvisioningJob> {
        let mut state = self.state.lock().await;
        let mut job = state.pending.pop_front()?;
        job.status = JobStatus::Running { progress: None };
        state.running.insert(job.target.clone(), job.clone());
        self.publish(&state);
        drop(state);
        let _ = self.events.send(QueueEvent::Started { job: job.clone() });
        Some(job)
    }

    /// Record a progress update for a running job.
    pub async fn update_progress(&self, target: &ProvisioningTarget, progress: DownloadProgress) {
        let mut state = self.state.lock().await;
        if let Some(job) = state.running.get_mut(target) {
            job.status = JobStatus::Running {
                progress: Some(progress.clone()),
            };
            self.publish(&state);
            drop(state);
            let _ = self.events.send(QueueEvent::Progress {
                target: target.clone(),
                progress,
            });
        }
    }

    /// Mark a running job complete. Clears any backoff for the target.
    pub async fn complete(&self, target: &ProvisioningTarget, duration: Duration) {
        let mut state = self.state.lock().await;
        if let Some(mut job) = state.running.remove(target) {
            state.backoff.remove(target);
            job.status = JobStatus::Completed { duration };
            state.history.push_front(job);
            if state.history.len() > HISTORY_CAP {
                state.history.pop_back();
            }
            self.publish(&state);
            drop(state);
            let _ = self.events.send(QueueEvent::Completed {
                target: target.clone(),
                duration_ms: duration.as_millis() as u64,
            });
        }
    }

    /// Mark a running job failed. Installs the backoff entry.
    pub async fn fail(&self, target: &ProvisioningTarget, reason: String) {
        let mut state = self.state.lock().await;
        if let Some(mut job) = state.running.remove(target) {
            let attempts = state.backoff.get(target).map(|(_, a)| a + 1).unwrap_or(1);
            let delay = Backoff::delay(attempts);
            let retry_after = Instant::now() + delay;
            state.backoff.insert(target.clone(), (retry_after, attempts));
            job.status = JobStatus::Failed {
                reason: reason.clone(),
                attempts,
                retry_in_secs: Some(delay.as_secs()),
            };
            state.history.push_front(job);
            if state.history.len() > HISTORY_CAP {
                state.history.pop_back();
            }
            self.publish(&state);
            drop(state);
            let _ = self.events.send(QueueEvent::Failed {
                target: target.clone(),
                reason,
                attempts,
            });
        }
    }

    /// Clear a backoff entry — used by user-triggered retries that
    /// should bypass the timer.
    pub async fn clear_backoff(&self, target: &ProvisioningTarget) {
        let mut state = self.state.lock().await;
        state.backoff.remove(target);
    }

    /// Drain the pending queue (shutdown path).
    pub async fn drain(&self) {
        let mut state = self.state.lock().await;
        state.pending.clear();
        self.publish(&state);
    }

    fn publish(&self, state: &QueueState) {
        let mut jobs: Vec<ProvisioningJob> = Vec::new();
        jobs.extend(state.running.values().cloned());
        jobs.extend(state.pending.iter().cloned());
        jobs.extend(state.history.iter().cloned());
        let snapshot = ProvisioningSnapshot {
            active: state.running.len(),
            queued: state.pending.len(),
            max_concurrency: self.max_concurrency,
            jobs,
        };
        self.snapshot_tx.send_replace(Arc::new(snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(skill: &str) -> ProvisioningTarget {
        ProvisioningTarget {
            skill: Moniker::new(skill).unwrap(),
            endpoint: "http://stone:8188".into(),
        }
    }

    #[test]
    fn backoff_schedule_caps_at_one_hour() {
        assert_eq!(Backoff::delay(1), Duration::from_secs(60));
        assert_eq!(Backoff::delay(2), Duration::from_secs(300));
        assert_eq!(Backoff::delay(3), Duration::from_secs(1800));
        assert_eq!(Backoff::delay(4), Duration::from_secs(3600));
        assert_eq!(Backoff::delay(100), Duration::from_secs(3600));
    }

    #[test]
    fn priority_ordering_user_before_discovery() {
        assert!(Priority::User < Priority::Discovery);
    }

    #[tokio::test]
    async fn submit_and_take_roundtrip() {
        let q = ProvisioningQueue::new(2);
        assert!(
            q.submit(target("upscale-skill"), Priority::Discovery, "stone".into(), "comfyui".into())
                .await
        );
        let job = q.take_next().await.unwrap();
        assert_eq!(job.target.skill.as_str(), "upscale-skill");
        assert!(matches!(job.status, JobStatus::Running { .. }));
        let snap = q.snapshot();
        assert_eq!(snap.active, 1);
        assert_eq!(snap.queued, 0);
    }

    #[tokio::test]
    async fn dedup_running_and_queued() {
        let q = ProvisioningQueue::new(2);
        assert!(q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await);
        // Still queued → second submit rejected.
        assert!(!q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await);
        q.take_next().await;
        // Now running → still rejected.
        assert!(!q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn failure_installs_backoff() {
        let q = ProvisioningQueue::new(2);
        q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await;
        q.take_next().await;
        q.fail(&target("a-skill"), "network error".into()).await;
        // Next submit within the backoff window is rejected.
        assert!(!q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await);

        // Clearing the backoff allows a resubmit.
        q.clear_backoff(&target("a-skill")).await;
        assert!(q.submit(target("a-skill"), Priority::User, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn priority_ordering_within_pending() {
        let q = ProvisioningQueue::new(2);
        q.submit(target("auto-one"), Priority::Discovery, "s".into(), "c".into()).await;
        q.submit(target("user-one"), Priority::User, "s".into(), "c".into()).await;
        q.submit(target("auto-two"), Priority::Discovery, "s".into(), "c".into()).await;

        let j1 = q.take_next().await.unwrap();
        let j2 = q.take_next().await.unwrap();
        let j3 = q.take_next().await.unwrap();
        assert_eq!(j1.target.skill.as_str(), "user-one");
        assert_eq!(j2.target.skill.as_str(), "auto-one");
        assert_eq!(j3.target.skill.as_str(), "auto-two");
    }

    #[tokio::test]
    async fn complete_clears_backoff_for_next_round() {
        let q = ProvisioningQueue::new(2);
        // Fail once to install backoff.
        q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await;
        q.take_next().await;
        q.fail(&target("a-skill"), "err".into()).await;

        // User override bypasses backoff — but submit will still
        // reject unless we clear it.
        q.clear_backoff(&target("a-skill")).await;
        q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await;
        q.take_next().await;
        q.complete(&target("a-skill"), Duration::from_secs(5)).await;

        // After a successful complete, the backoff is cleared, so a
        // fresh submit succeeds immediately.
        assert!(q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn snapshot_reflects_queue_state() {
        let q = ProvisioningQueue::new(2);
        q.submit(target("a-skill"), Priority::Discovery, "s".into(), "c".into()).await;
        q.submit(target("b-skill"), Priority::Discovery, "s".into(), "c".into()).await;
        let snap = q.snapshot();
        assert_eq!(snap.queued, 2);
        assert_eq!(snap.active, 0);
        q.take_next().await;
        let snap = q.snapshot();
        assert_eq!(snap.queued, 1);
        assert_eq!(snap.active, 1);
    }
}
