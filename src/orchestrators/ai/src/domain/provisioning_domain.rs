//! Provisioning domain — job queue state + snapshot (ORCH-0024).
//!
//! Owns mutable state privately. Publishes immutable snapshots via watch.
//! Discovery submits jobs here. The worker in skills/queue.rs consumes them.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{watch, Mutex, Notify};

use super::provisioning::*;

/// Maximum number of completed/failed jobs to keep in history.
const HISTORY_CAP: usize = 50;

/// Default max concurrent provisioning downloads.
pub const DEFAULT_CONCURRENCY: usize = 2;

// ── Domain ───────────────────────────────────────────────────

pub struct ProvisioningDomain {
    state: Mutex<QueueState>,
    tx: watch::Sender<Arc<ProvisioningSnapshot>>,
    /// Notified when a new job is submitted — wakes the worker.
    notify: Arc<Notify>,
    concurrency: usize,
}

struct QueueState {
    /// Pending jobs, sorted by priority then submission time.
    pending: VecDeque<ProvisioningJob>,
    /// Currently executing jobs, keyed by target for dedup.
    running: HashMap<ProvisioningTarget, ProvisioningJob>,
    /// Recently completed/failed jobs (ring buffer).
    history: VecDeque<ProvisioningJob>,
    /// Targets on backoff — not eligible for resubmission until the Instant.
    backoff: HashMap<ProvisioningTarget, (Instant, u32)>,
}

impl ProvisioningDomain {
    pub fn new(concurrency: usize) -> (Self, watch::Receiver<Arc<ProvisioningSnapshot>>) {
        let (tx, rx) = watch::channel(Arc::new(ProvisioningSnapshot {
            jobs: Vec::new(),
            active: 0,
            queued: 0,
            max_concurrency: concurrency,
        }));

        let domain = Self {
            state: Mutex::new(QueueState {
                pending: VecDeque::new(),
                running: HashMap::new(),
                history: VecDeque::new(),
                backoff: HashMap::new(),
            }),
            tx,
            notify: Arc::new(Notify::new()),
            concurrency,
        };

        (domain, rx)
    }

    /// Get the notification handle (for the worker to wait on).
    pub fn notifier(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// Get the configured max concurrency.
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Submit a provisioning job. Returns false if deduplicated.
    pub async fn submit(
        &self,
        target: ProvisioningTarget,
        priority: Priority,
        stone_name: String,
        provider: String,
    ) -> bool {
        let mut state = self.state.lock().await;

        // Dedup: already running
        if state.running.contains_key(&target) {
            return false;
        }

        // Dedup: already queued
        if state.pending.iter().any(|j| j.target == target) {
            return false;
        }

        // Dedup: on backoff and not yet eligible
        if let Some((retry_after, _)) = state.backoff.get(&target) {
            if Instant::now() < *retry_after {
                return false;
            }
            state.backoff.remove(&target);
        }

        let job = ProvisioningJob::new(target, priority, stone_name, provider);

        // Insert sorted: User priority before Discovery
        let insert_pos = state.pending.partition_point(|j| j.priority <= priority);
        state.pending.insert(insert_pos, job);

        self.publish(&state);
        drop(state);

        self.notify.notify_one();
        true
    }

    /// Called by the worker to take the next eligible job.
    pub async fn take_next(&self) -> Option<ProvisioningJob> {
        let mut state = self.state.lock().await;
        if let Some(mut job) = state.pending.pop_front() {
            job.status = JobStatus::Running { progress: None };
            state.running.insert(job.target.clone(), job.clone());
            self.publish(&state);
            Some(job)
        } else {
            None
        }
    }

    /// Mark a job as completed.
    pub async fn complete(&self, target: &ProvisioningTarget, duration: Duration) {
        let mut state = self.state.lock().await;
        if let Some(mut job) = state.running.remove(target) {
            // Clear any previous backoff
            state.backoff.remove(target);

            job.status = JobStatus::Completed { duration };
            state.history.push_front(job);
            if state.history.len() > HISTORY_CAP {
                state.history.pop_back();
            }
            self.publish(&state);
        }
    }

    /// Mark a job as failed. Computes backoff.
    pub async fn fail(&self, target: &ProvisioningTarget, reason: String) {
        let mut state = self.state.lock().await;
        if let Some(mut job) = state.running.remove(target) {
            let attempts = state.backoff.get(target)
                .map(|(_, a)| a + 1)
                .unwrap_or(1);

            let delay = Backoff::delay(attempts);
            let retry_after = Instant::now() + delay;
            state.backoff.insert(target.clone(), (retry_after, attempts));

            job.status = JobStatus::Failed {
                reason,
                attempts,
                retry_in_secs: Some(delay.as_secs()),
            };
            state.history.push_front(job);
            if state.history.len() > HISTORY_CAP {
                state.history.pop_back();
            }
            self.publish(&state);
        }
    }

    /// Update download progress for a running job.
    pub async fn update_progress(&self, target: &ProvisioningTarget, progress: DownloadProgress) {
        let mut state = self.state.lock().await;
        if let Some(job) = state.running.get_mut(target) {
            job.status = JobStatus::Running { progress: Some(progress) };
            self.publish(&state);
        }
    }

    /// Clear backoff for a target (user-triggered retry).
    pub async fn clear_backoff(&self, target: &ProvisioningTarget) {
        let mut state = self.state.lock().await;
        state.backoff.remove(target);
    }

    /// Drain all pending jobs (for shutdown).
    pub async fn drain(&self) {
        let mut state = self.state.lock().await;
        state.pending.clear();
        self.publish(&state);
    }

    /// Current count of active (running) jobs.
    pub async fn active_count(&self) -> usize {
        self.state.lock().await.running.len()
    }

    /// Get a snapshot for API responses.
    pub fn snapshot(&self) -> watch::Ref<'_, Arc<ProvisioningSnapshot>> {
        self.tx.borrow()
    }

    /// Publish an updated snapshot via the watch channel.
    fn publish(&self, state: &QueueState) {
        let mut jobs: Vec<ProvisioningJob> = Vec::new();
        jobs.extend(state.running.values().cloned());
        jobs.extend(state.pending.iter().cloned());
        jobs.extend(state.history.iter().cloned());

        let snapshot = ProvisioningSnapshot {
            active: state.running.len(),
            queued: state.pending.len(),
            max_concurrency: self.concurrency,
            jobs,
        };

        self.tx.send_modify(|s| *s = Arc::new(snapshot));
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn target(skill: &str) -> ProvisioningTarget {
        ProvisioningTarget {
            skill: skill.into(),
            endpoint: "http://localhost:8188".into(),
        }
    }

    #[tokio::test]
    async fn submit_and_take() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        assert!(domain.submit(target("a"), Priority::Discovery, "stone".into(), "comfyui".into()).await);

        let job = domain.take_next().await.unwrap();
        assert_eq!(job.target.skill, "a");
        assert!(matches!(job.status, JobStatus::Running { .. }));
    }

    #[tokio::test]
    async fn dedup_running() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.take_next().await; // now running

        // Can't submit same target while running
        assert!(!domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn dedup_queued() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        assert!(!domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn backoff_on_failure() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.take_next().await;
        domain.fail(&target("a"), "network error".into()).await;

        // Should be rejected — on backoff
        assert!(!domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn clear_backoff_allows_resubmit() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.take_next().await;
        domain.fail(&target("a"), "error".into()).await;

        domain.clear_backoff(&target("a")).await;
        assert!(domain.submit(target("a"), Priority::User, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn priority_ordering() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("auto1"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.submit(target("user1"), Priority::User, "s".into(), "c".into()).await;
        domain.submit(target("auto2"), Priority::Discovery, "s".into(), "c".into()).await;

        let j1 = domain.take_next().await.unwrap();
        let j2 = domain.take_next().await.unwrap();
        let j3 = domain.take_next().await.unwrap();

        assert_eq!(j1.target.skill, "user1");
        assert_eq!(j2.target.skill, "auto1");
        assert_eq!(j3.target.skill, "auto2");
    }

    #[tokio::test]
    async fn complete_clears_backoff() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        // Fail first, creating backoff
        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.take_next().await;
        domain.fail(&target("a"), "err".into()).await;

        // Clear backoff and succeed
        domain.clear_backoff(&target("a")).await;
        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.take_next().await;
        domain.complete(&target("a"), Duration::from_secs(5)).await;

        // Should be submittable again (backoff cleared on complete)
        assert!(domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await);
    }

    #[tokio::test]
    async fn snapshot_reflects_state() {
        let (domain, _rx) = ProvisioningDomain::new(2);

        domain.submit(target("a"), Priority::Discovery, "s".into(), "c".into()).await;
        domain.submit(target("b"), Priority::Discovery, "s".into(), "c".into()).await;

        let snap = domain.snapshot().clone();
        assert_eq!(snap.queued, 2);
        assert_eq!(snap.active, 0);

        domain.take_next().await;
        let snap = domain.snapshot().clone();
        assert_eq!(snap.queued, 1);
        assert_eq!(snap.active, 1);
    }
}
