//! Disk-backed job store.
//!
//! Layout: `{data_dir}/jobs/{job_id}.json`. Each file is a single
//! `JobFile` record atomically overwritten on every state change.
//!
//! Jobs are ephemeral but durable across restarts — a long-running
//! async ComfyUI workflow survives an orchestrator reboot and its
//! callers can poll `/v1/jobs/{id}` throughout.
//!
//! Lifetime discipline (§ADR):
//! - Running jobs are never evicted.
//! - Terminal jobs older than
//!   [`TERMINAL_GRACE`] are evicted by the `sweep` method.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};

use crate::domain::ids::{CorrelationId, JobId, ProviderName};
use crate::domain::jobs::{
    Job, JobCategory, JobFilter, JobState, JobStore, JobStoreError, JobTerminalEvent, Progress,
};
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::request::Action;

/// Terminal jobs live at least this long before the sweeper evicts them.
pub const TERMINAL_GRACE: ChronoDuration = ChronoDuration::hours(24);

// ── On-disk format ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobFile {
    id: String,
    correlation_id: String,
    category: JobCategory,
    owner: Option<String>,
    action: Option<ActionFile>,
    state: JobState,
    progress: Option<Progress>,
    eta_seconds: Option<u64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    terminal_at: Option<DateTime<Utc>>,
    metadata: Value,
    result: Option<Value>,
    error: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActionFile {
    primitive: Primitive,
    skill: Option<String>,
}

impl JobFile {
    fn from_job(job: &Job) -> Self {
        Self {
            id: job.id.as_str().to_string(),
            correlation_id: job.correlation_id.as_str().to_string(),
            category: job.category,
            owner: job.owner.as_ref().map(|p| p.as_str().to_string()),
            action: job.action.as_ref().map(|a| ActionFile {
                primitive: a.primitive,
                skill: a.skill.as_ref().map(|s| s.as_str().to_string()),
            }),
            state: job.state,
            progress: job.progress.clone(),
            eta_seconds: job.eta_seconds,
            created_at: job.created_at,
            updated_at: job.updated_at,
            terminal_at: job.terminal_at,
            metadata: job.metadata.clone(),
            result: job.result.as_ref().map(|r| r.to_nested()),
            error: job.error.clone(),
        }
    }

    fn into_job(self) -> Result<Job, JobStoreError> {
        let action = match self.action {
            Some(a) => Some(Action {
                primitive: a.primitive,
                skill: a
                    .skill
                    .map(|s| Moniker::new(s).map_err(|e| JobStoreError::Storage(e.to_string())))
                    .transpose()?,
            }),
            None => None,
        };
        let result = match self.result {
            Some(val) => Some(Output::from_nested(val).map_err(|e| {
                JobStoreError::Storage(format!("deserialize output: {e}"))
            })?),
            None => None,
        };
        Ok(Job {
            id: JobId::from_string(self.id),
            correlation_id: CorrelationId::from_string(self.correlation_id),
            category: self.category,
            owner: self.owner.map(ProviderName::new),
            action,
            state: self.state,
            progress: self.progress,
            eta_seconds: self.eta_seconds,
            created_at: self.created_at,
            updated_at: self.updated_at,
            terminal_at: self.terminal_at,
            metadata: self.metadata,
            result,
            error: self.error,
        })
    }
}

// ── Store ─────────────────────────────────────────────────────

pub struct DiskJobStore {
    root: PathBuf,
    jobs: RwLock<HashMap<String, Job>>,
    terminal_tx: broadcast::Sender<JobTerminalEvent>,
}

impl DiskJobStore {
    pub async fn load(data_dir: &Path) -> Result<Arc<Self>, JobStoreError> {
        let root = data_dir.join("jobs");
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(|e| JobStoreError::Storage(e.to_string()))?;
        let (terminal_tx, _) = broadcast::channel(256);
        let store = Arc::new(Self {
            root,
            jobs: RwLock::new(HashMap::new()),
            terminal_tx,
        });
        store.scan().await?;
        Ok(store)
    }

    /// Publish a terminal event. Errors (no subscribers) are ignored —
    /// the channel is advisory.
    fn publish_terminal(&self, id: JobId, state: JobState) {
        let _ = self.terminal_tx.send(JobTerminalEvent { id, state });
    }

    async fn scan(&self) -> Result<(), JobStoreError> {
        let mut read = match tokio::fs::read_dir(&self.root).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(JobStoreError::Storage(e.to_string())),
        };
        let mut jobs: HashMap<String, Job> = HashMap::new();
        while let Ok(Some(dirent)) = read.next_entry().await {
            let path = dirent.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read(&path).await {
                Ok(bytes) => match serde_json::from_slice::<JobFile>(&bytes) {
                    Ok(file) => match file.into_job() {
                        Ok(job) => {
                            jobs.insert(job.id.as_str().to_string(), job);
                        }
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "failed to decode job");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "failed to parse job");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to read job");
                }
            }
        }
        *self.jobs.write().await = jobs;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    async fn persist(&self, job: &Job) -> Result<(), JobStoreError> {
        let path = self.path_for(job.id.as_str());
        let file = JobFile::from_job(job);
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| JobStoreError::Storage(format!("serialize: {e}")))?;
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| JobStoreError::Storage(format!("write {}: {e}", path.display())))
    }

    async fn apply<F>(&self, id: &JobId, mutate: F) -> Result<(), JobStoreError>
    where
        F: FnOnce(&mut Job),
    {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(id.as_str())
            .ok_or_else(|| JobStoreError::NotFound(id.clone()))?;
        mutate(job);
        job.updated_at = Utc::now();
        let cloned = job.clone();
        drop(jobs);
        self.persist(&cloned).await
    }
}

#[async_trait]
impl JobStore for DiskJobStore {
    async fn create(
        &self,
        correlation_id: CorrelationId,
        category: JobCategory,
        owner: Option<ProviderName>,
        action: Option<Action>,
        metadata: Value,
    ) -> Result<Job, JobStoreError> {
        let id = JobId::generate();
        let mut job = Job::new(id.clone(), correlation_id, category, owner, action, Utc::now());
        job.metadata = metadata;
        self.persist(&job).await?;
        self.jobs.write().await.insert(id.as_str().to_string(), job.clone());
        Ok(job)
    }

    async fn get(&self, id: &JobId) -> Result<Option<Job>, JobStoreError> {
        Ok(self.jobs.read().await.get(id.as_str()).cloned())
    }

    async fn list(&self, filter: JobFilter) -> Result<Vec<Job>, JobStoreError> {
        let jobs = self.jobs.read().await;
        let mut out: Vec<Job> = jobs
            .values()
            .filter(|j| {
                filter
                    .category
                    .map(|c| j.category == c)
                    .unwrap_or(true)
                    && filter.state.map(|s| j.state == s).unwrap_or(true)
                    && filter
                        .owner
                        .as_ref()
                        .map(|o| j.owner.as_ref() == Some(o))
                        .unwrap_or(true)
                    && filter
                        .action_dotted
                        .as_ref()
                        .map(|d| j.action.as_ref().map(|a| a.dotted() == *d).unwrap_or(false))
                        .unwrap_or(true)
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    async fn update_state(&self, id: &JobId, state: JobState) -> Result<(), JobStoreError> {
        self.apply(id, |job| {
            job.state = state;
            if state.is_terminal() {
                job.terminal_at = Some(Utc::now());
            }
        })
        .await?;
        if state.is_terminal() {
            self.publish_terminal(id.clone(), state);
        }
        Ok(())
    }

    async fn update_progress(
        &self,
        id: &JobId,
        progress: Progress,
    ) -> Result<(), JobStoreError> {
        self.apply(id, |job| {
            job.progress = Some(progress);
        })
        .await
    }

    async fn update_eta(&self, id: &JobId, eta_seconds: u64) -> Result<(), JobStoreError> {
        self.apply(id, |job| job.eta_seconds = Some(eta_seconds)).await
    }

    async fn complete(&self, id: &JobId, result: Output) -> Result<(), JobStoreError> {
        self.apply(id, move |job| {
            job.state = JobState::Done;
            job.terminal_at = Some(Utc::now());
            job.result = Some(result);
        })
        .await?;
        self.publish_terminal(id.clone(), JobState::Done);
        Ok(())
    }

    async fn fail(&self, id: &JobId, error: Value) -> Result<(), JobStoreError> {
        self.apply(id, move |job| {
            job.state = JobState::Failed;
            job.terminal_at = Some(Utc::now());
            job.error = Some(error);
        })
        .await?;
        self.publish_terminal(id.clone(), JobState::Failed);
        Ok(())
    }

    async fn cancel(&self, id: &JobId) -> Result<(), JobStoreError> {
        let fired = {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .get_mut(id.as_str())
                .ok_or_else(|| JobStoreError::NotFound(id.clone()))?;
            if job.state.is_terminal() {
                false
            } else {
                job.state = JobState::Cancelled;
                job.updated_at = Utc::now();
                job.terminal_at = Some(Utc::now());
                let cloned = job.clone();
                drop(jobs);
                self.persist(&cloned).await?;
                true
            }
        };
        if fired {
            self.publish_terminal(id.clone(), JobState::Cancelled);
        }
        Ok(())
    }

    async fn sweep(&self, now: DateTime<Utc>) -> Result<u64, JobStoreError> {
        let to_remove: Vec<String> = {
            let jobs = self.jobs.read().await;
            jobs.values()
                .filter(|j| {
                    j.state.is_terminal()
                        && j.terminal_at
                            .map(|t| (now - t) > TERMINAL_GRACE)
                            .unwrap_or(false)
                })
                .map(|j| j.id.as_str().to_string())
                .collect()
        };
        let count = to_remove.len() as u64;
        let mut jobs = self.jobs.write().await;
        for id in &to_remove {
            jobs.remove(id);
            let _ = tokio::fs::remove_file(self.path_for(id)).await;
        }
        Ok(count)
    }

    fn subscribe_terminal(&self) -> broadcast::Receiver<JobTerminalEvent> {
        self.terminal_tx.subscribe()
    }
}
