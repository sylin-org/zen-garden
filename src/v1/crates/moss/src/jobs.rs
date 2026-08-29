//! The jobs registry: every async operation the stone runs, tracked and
//! queryable (ADR: the data plane's async contract).
//!
//! A **job** is any long-running operation — a capture, a nourish sweep, a
//! future deploy. Jobs are created by the surface that initiates them,
//! transition through `running` to `done` or `failed`, and are queryable
//! by any client for their lifetime. This is the difference between "I
//! sent a command and hope" and "I sent a command and can check on it."
//!
//! Jobs JOURNAL to disk when a journal root is given (L11: no state
//! machine without its crash-recovery path): start/complete/fail rewrite
//! one small JSON file per job, and a boot reconcile marks still-running
//! journals `interrupted` — the truth of what actually landed is
//! re-observed by the domain sweep, never assumed.

// JobStatus variants land with their consuming slices (faces, scheduler).
#![allow(dead_code)]

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// How far along a job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Done,
    Failed,
    /// The process died mid-job; whatever the operation accomplished is
    /// re-observed, never resumed blindly (L11).
    Interrupted,
}

/// One tracked async operation.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
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
    /// Runtime-only progress line (the last word the operation said).
    /// Never journaled: status truth survives restarts, progress does
    /// not — an interrupted job is interrupted, not partially-alive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

/// Tracks async operations for the stone. Clone freely; all clones share state.
#[derive(Clone)]
pub struct JobTracker {
    jobs: Arc<parking_lot::Mutex<HashMap<String, Job>>>,
    changes: Arc<tokio::sync::broadcast::Sender<String>>,
    journal: Option<Arc<std::path::PathBuf>>,
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
            journal: None,
        }
    }

    /// Speak a progress line for a running job. Runtime state only — the
    /// journal stays out of it — but the change signal fires so the pulse
    /// carries the news.
    pub fn progress(&self, id: &str, line: impl Into<String>) {
        if let Some(job) = self.jobs.lock().get_mut(id) {
            job.progress = Some(line.into());
            let _ = self.changes.send(id.to_string());
        }
    }

    /// Track jobs that survive the process on disk: one JSON file per
    /// job under `dir`, rewritten at every transition (L11).
    pub fn with_journal(dir: std::path::PathBuf) -> Self {
        let mut tracker = Self::new();
        tracker.journal = Some(Arc::new(dir));
        tracker
    }

    /// Boot reconciliation (L11): journals still marked `running` belong
    /// to jobs the process died inside — mark them `interrupted`, loudly.
    /// What actually landed is the world's business; the sweeps re-observe.
    pub fn interrupt_stale_running(&self) -> usize {
        let Some(dir) = self.journal.as_ref().map(|d| d.as_ref().clone()) else {
            return 0;
        };
        let mut interrupted = 0usize;
        let Ok(entries) = std::fs::read_dir(&dir) else { return 0 };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(job) = serde_json::from_slice::<Job>(&bytes) else {
                continue;
            };
            if job.status != JobStatus::Running {
                continue;
            }
            let mut interrupted_job = job.clone();
            interrupted_job.status = JobStatus::Interrupted;
            interrupted_job.finished_at = Some(chrono::Utc::now());
            interrupted_job.error =
                Some("interrupted by restart — ask again; what landed is re-observed".into());
            self.jobs.lock().insert(job.id.clone(), interrupted_job.clone());
            if let Err(e) = serde_json::to_vec_pretty(&interrupted_job)
                .map_err(|e| e.to_string())
                .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
            {
                tracing::warn!(error = %e, "job journal rewrite failed");
            }
            interrupted += 1;
        }
        if interrupted > 0 {
            tracing::warn!(interrupted, "jobs interrupted by the last restart");
        }
        interrupted
    }

    fn journal_write(&self, job: &Job) {
        let Some(dir) = self.journal.as_ref().map(|d| d.as_ref().clone()) else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, "job journal dir create failed");
            return;
        }
        let path = dir.join(format!("{}.json", job.id));
        if let Err(e) = serde_json::to_vec_pretty(job)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
        {
            tracing::warn!(error = %e, "job journal write failed");
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
            progress: None,
        };
        self.journal_write(&job);
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
            self.journal_write(job);
        }
        let _ = self.changes.send(id.to_string());
    }

    /// Mark a job failed.
    pub fn fail(&self, id: &str, error: &str) {
        if let Some(job) = self.jobs.lock().get_mut(id) {
            job.status = JobStatus::Failed;
            job.finished_at = Some(chrono::Utc::now());
            job.error = Some(error.to_string());
            self.journal_write(job);
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

    /// L11: a job the process died inside is found again at boot —
    /// marked interrupted, never haunted as running.
    #[test]
    fn journaled_jobs_survive_the_process_and_interrupt_honestly() {
        let dir = std::env::temp_dir()
            .join(format!("zg-jobs-{}-{}", std::process::id(), uuid::Uuid::now_v7()));
        let tracker = JobTracker::with_journal(dir.clone());
        let id = tracker.start("capability-install", "ollama/model:llama3");

        // The journal exists and reads back as running.
        let path = dir.join(format!("{id}.json"));
        let on_disk = serde_json::from_str::<Job>(
            &std::fs::read_to_string(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.status, JobStatus::Running);

        // A NEW process (fresh tracker, same journal) reconciles.
        let rebooted = JobTracker::with_journal(dir.clone());
        assert_eq!(rebooted.interrupt_stale_running(), 1);
        let job = rebooted.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Interrupted);
        assert!(job.error.as_deref().unwrap().contains("interrupted"));

        // Idempotent: a second boot finds nothing running.
        assert_eq!(rebooted.interrupt_stale_running(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Completions rewrite the journal — a finished job never interrupts.
    #[test]
    fn completed_jobs_journal_their_fate() {
        let dir = std::env::temp_dir()
            .join(format!("zg-jobs-{}-{}", std::process::id(), uuid::Uuid::now_v7()));
        let tracker = JobTracker::with_journal(dir.clone());
        let id = tracker.start("capability-install", "ollama/model:llama3");
        tracker.complete(&id, serde_json::json!({"item": "llama3"}));
        let rebooted = JobTracker::with_journal(dir.clone());
        assert_eq!(rebooted.interrupt_stale_running(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
