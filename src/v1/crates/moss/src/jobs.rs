//! The jobs registry: every async operation the stone runs, tracked and
//! queryable (ADR: the data plane's async contract).
//!
//! A **job** is any long-running operation — a capture, a nourish sweep, a
//! future deploy. Jobs are created by the surface that initiates them,
//! transition through `running` to `done` or `failed`, and are queryable
//! by any client for their lifetime. This is the difference between "I
//! sent a command and hope" and "I sent a command and can check on it."
//!
//! Jobs live in memory for now (they are runtime state, not on-media
//! identity); persistence arrives when restart-resume of interrupted jobs
//! becomes a requirement.

// JobStatus variants land with their consuming slices (faces, scheduler).
#![allow(dead_code)]

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// How far along a job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Done,
    Failed,
}

/// One tracked async operation.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: String,
    /// What kind of operation: "capture", "nourish", "replant", ...
    pub kind: String,
    /// What it operates on (an offering FQN, a bank FQN, ...).
    pub subject: String,
    pub status: JobStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// Tracks async operations for the stone. Clone freely; all clones share state.
#[derive(Clone)]
pub struct JobTracker {
    jobs: Arc<parking_lot::Mutex<HashMap<String, Job>>>,
    changes: Arc<tokio::sync::broadcast::Sender<String>>,
}

impl Default for JobTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl JobTracker {
    pub fn new() -> Self {
        let (changes, _) = tokio::sync::broadcast::channel(64);
        Self {
            jobs: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            changes: Arc::new(changes),
        }
    }

    /// Subscribe to job-id changes (the id string is the signal; fetch
    /// the job for details).
    pub fn changes(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.changes.subscribe()
    }

    /// Register a new running job. Returns its id.
    pub fn start(&self, kind: &str, subject: &str) -> String {
        let id = uuid::Uuid::now_v7().to_string();
        let job = Job {
            id: id.clone(),
            kind: kind.to_string(),
            subject: subject.to_string(),
            status: JobStatus::Running,
            started_at: chrono::Utc::now(),
            finished_at: None,
            error: None,
            result: None,
        };
        self.jobs.lock().insert(id.clone(), job);
        let _ = self.changes.send(id.clone());
        id
    }

    /// Mark a job done with an optional result payload.
    pub fn complete(&self, id: &str, result: serde_json::Value) {
        if let Some(job) = self.jobs.lock().get_mut(id) {
            job.status = JobStatus::Done;
            job.finished_at = Some(chrono::Utc::now());
            job.result = Some(result);
        }
        let _ = self.changes.send(id.to_string());
    }

    /// Mark a job failed.
    pub fn fail(&self, id: &str, error: &str) {
        if let Some(job) = self.jobs.lock().get_mut(id) {
            job.status = JobStatus::Failed;
            job.finished_at = Some(chrono::Utc::now());
            job.error = Some(error.to_string());
        }
        let _ = self.changes.send(id.to_string());
    }

    /// Every tracked job, newest first.
    pub fn list(&self) -> Vec<Job> {
        let mut jobs: Vec<Job> = self.jobs.lock().values().cloned().collect();
        jobs.sort_by_key(|j| std::cmp::Reverse(j.started_at));
        jobs
    }

    /// One job by id.
    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.lock().get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn job_lifecycle_transitions() {
        let tracker = JobTracker::new();
        let id = tracker.start("capture", "redis::default");

        let job = tracker.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.kind, "capture");
        assert_eq!(job.subject, "redis::default");

        tracker.complete(&id, serde_json::json!({"files": 12}));
        let job = tracker.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.result.as_ref().unwrap()["files"], 12);
    }

    #[test]
    fn failed_jobs_carry_the_error() {
        let tracker = JobTracker::new();
        let id = tracker.start("nourish", "mongodb::default");
        tracker.fail(&id, "the network said no");

        let job = tracker.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.as_deref(), Some("the network said no"));
    }

    #[test]
    fn jobs_list_newest_first() {
        let tracker = JobTracker::new();
        let a = tracker.start("capture", "a");
        let b = tracker.start("capture", "b");
        let jobs = tracker.list();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, b, "newest first");
        assert_eq!(jobs[1].id, a);
        let _ = (a, b);
    }

    #[test]
    fn jobs_serialize_with_status_lowercase() {
        let tracker = JobTracker::new();
        let id = tracker.start("capture", "test");
        let job = tracker.get(&id).unwrap();
        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "running");

        tracker.complete(&id, serde_json::json!({}));
        let job = tracker.get(&id).unwrap();
        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "done");
    }

    // Verify Deserialize isn't accidentally required (jobs are serialize-only
    // for now; deserialization arrives with persistence).
    #[test]
    fn jobs_are_serialize_only_for_now() {
        let tracker = JobTracker::new();
        let id = tracker.start("capture", "x");
        let job = tracker.get(&id).unwrap();
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("\"kind\":\"capture\""));
    }
}
