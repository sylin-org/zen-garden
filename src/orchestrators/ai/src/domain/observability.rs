//! Observability domain — metrics, demand, jobs (ORCH-0020).
//!
//! Owns MetricsEngine, DemandLedger, and orchestrator jobs.
//! Publishes job snapshots via watch. Demand shares computed on demand.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use super::demand::DemandLedger;
use super::metrics::MetricsEngine;
use super::types::{
    Capability, JobKind, JobStatus, MetricEvent, MetricsSnapshot, OrchestratorJob,
};

// ── Snapshot ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ObservabilitySnapshot {
    pub jobs: Arc<Vec<OrchestratorJob>>,
}

impl ObservabilitySnapshot {
    pub fn empty() -> Self {
        Self {
            jobs: Arc::new(Vec::new()),
        }
    }
}

// ── Domain ─────────────────────────────────────────────────────

pub struct ObservabilityDomain {
    state: Mutex<ObservabilityState>,
    tx: watch::Sender<Arc<ObservabilitySnapshot>>,
}

struct ObservabilityState {
    metrics: MetricsEngine,
    demand: DemandLedger,
    jobs: VecDeque<OrchestratorJob>,
}

const MAX_JOBS: usize = 20;

impl ObservabilityDomain {
    pub fn new(
        tx: watch::Sender<Arc<ObservabilitySnapshot>>,
        metrics_enabled: bool,
    ) -> Self {
        let mut engine = MetricsEngine::new();
        engine.enabled = metrics_enabled;

        Self {
            state: Mutex::new(ObservabilityState {
                metrics: engine,
                demand: DemandLedger::new(),
                jobs: VecDeque::with_capacity(MAX_JOBS),
            }),
            tx,
        }
    }

    pub fn snapshot(&self) -> watch::Ref<'_, Arc<ObservabilitySnapshot>> {
        self.tx.borrow()
    }

    // ── Metrics ────────────────────────────────────────────────

    /// Process a metric event (called by metrics_processor task).
    pub async fn process_event(&self, event: MetricEvent) {
        let mut state = self.state.lock().await;
        state.metrics.process_event(event);
    }

    /// Record a demand observation (called by metrics_processor for request events).
    pub async fn record_demand(
        &self,
        capability: Capability,
        model: &str,
        stone: &str,
        tokens_out: u64,
        eval_duration_ns: u64,
    ) {
        let mut state = self.state.lock().await;
        state.demand.record_request(
            std::time::Instant::now(),
            capability,
            model,
            stone,
            tokens_out,
            eval_duration_ns,
        );
    }

    /// Get recent demand shares for routing (brief lock).
    pub async fn demand_shares(&self, window_secs: u64) -> HashMap<String, f64> {
        let state = self.state.lock().await;
        state.metrics.demand_shares(window_secs)
    }

    /// Restore metrics from persisted snapshot.
    pub async fn restore_metrics(&self, snapshot: MetricsSnapshot) {
        let mut state = self.state.lock().await;
        state.metrics.restore_from_snapshot(snapshot);
    }

    /// Get metrics snapshot for persistence flush.
    pub async fn metrics_snapshot(&self) -> MetricsSnapshot {
        let state = self.state.lock().await;
        state.metrics.snapshot()
    }

    /// Check if metrics collection is enabled.
    pub async fn metrics_enabled(&self) -> bool {
        let state = self.state.lock().await;
        state.metrics.enabled
    }

    /// Toggle metrics collection.
    pub async fn set_metrics_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().await;
        state.metrics.enabled = enabled;
    }

    // ── Jobs ───────────────────────────────────────────────────

    pub async fn create_job(&self, kind: JobKind) -> String {
        let id = format!("job-{}", chrono::Utc::now().timestamp_millis());
        let job = OrchestratorJob {
            id: id.clone(),
            kind,
            status: JobStatus::Queued,
            progress: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            error: None,
        };

        let mut state = self.state.lock().await;
        if state.jobs.len() >= MAX_JOBS {
            state.jobs.pop_front();
        }
        state.jobs.push_back(job);
        self.publish(&state);

        id
    }

    pub async fn update_job(&self, id: &str, status: JobStatus, progress: Option<String>) {
        let mut state = self.state.lock().await;
        if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
            job.status = status;
            if progress.is_some() {
                job.progress = progress;
            }
        }
        self.publish(&state);
    }

    pub async fn complete_job(&self, id: &str) {
        let mut state = self.state.lock().await;
        if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Completed;
            job.completed_at = Some(chrono::Utc::now());
        }
        self.publish(&state);
    }

    pub async fn fail_job(&self, id: &str, error: &str) {
        let mut state = self.state.lock().await;
        if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Failed;
            job.completed_at = Some(chrono::Utc::now());
            job.error = Some(error.to_string());
        }
        self.publish(&state);
    }

    fn publish(&self, state: &ObservabilityState) {
        let jobs: Vec<_> = state.jobs.iter().rev().cloned().collect();
        let snapshot = Arc::new(ObservabilitySnapshot {
            jobs: Arc::new(jobs),
        });
        self.tx.send_modify(|current| *current = snapshot);
    }
}
