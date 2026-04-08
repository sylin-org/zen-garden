//! Job model — tracked units of work with observable progress.
//!
//! Jobs are ephemeral operational state. A job record exists for as
//! long as someone observing the system has reason to care about it;
//! after that it is evicted. Operators needing long-term history rely
//! on metrics and log aggregation, not on job queries.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::domain::ids::{CorrelationId, JobId, ProviderName};
use crate::domain::output::Output;
use crate::domain::provider::ProviderError;
use crate::domain::request::Action;

// ── Value objects ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub correlation_id: CorrelationId,
    pub category: JobCategory,
    pub owner: Option<ProviderName>,
    pub action: Option<Action>,
    pub state: JobState,
    pub progress: Option<Progress>,
    pub eta_seconds: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub metadata: Value,
    pub result: Option<Output>,
    pub error: Option<Value>,
}

impl Job {
    pub fn new(
        id: JobId,
        correlation_id: CorrelationId,
        category: JobCategory,
        owner: Option<ProviderName>,
        action: Option<Action>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            correlation_id,
            category,
            owner,
            action,
            state: JobState::Queued,
            progress: None,
            eta_seconds: None,
            created_at: now,
            updated_at: now,
            terminal_at: None,
            metadata: Value::Null,
            result: None,
            error: None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            JobState::Done | JobState::Failed | JobState::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobCategory {
    /// API-initiated async request.
    Api,
    /// Provider-initiated background work.
    Provider,
    /// Orchestrator-initiated maintenance.
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub current: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub category: Option<JobCategory>,
    pub state: Option<JobState>,
    pub owner: Option<ProviderName>,
    pub action_dotted: Option<String>,
}

// ── JobStore trait ────────────────────────────────────────────

/// Event published on every terminal transition. Consumed by the
/// media-reservation reaper and any dashboard wanting a change feed.
#[derive(Debug, Clone)]
pub struct JobTerminalEvent {
    pub id: JobId,
    pub state: JobState,
}

#[async_trait]
pub trait JobStore: Send + Sync + 'static {
    /// Create a new job record in the `Queued` state.
    async fn create(
        &self,
        correlation_id: CorrelationId,
        category: JobCategory,
        owner: Option<ProviderName>,
        action: Option<Action>,
        metadata: Value,
    ) -> Result<Job, JobStoreError>;

    async fn get(&self, id: &JobId) -> Result<Option<Job>, JobStoreError>;
    async fn list(&self, filter: JobFilter) -> Result<Vec<Job>, JobStoreError>;

    async fn update_state(&self, id: &JobId, state: JobState) -> Result<(), JobStoreError>;
    async fn update_progress(&self, id: &JobId, progress: Progress) -> Result<(), JobStoreError>;
    async fn update_eta(&self, id: &JobId, eta_seconds: u64) -> Result<(), JobStoreError>;
    async fn complete(&self, id: &JobId, result: Output) -> Result<(), JobStoreError>;
    async fn fail(&self, id: &JobId, error: Value) -> Result<(), JobStoreError>;
    async fn cancel(&self, id: &JobId) -> Result<(), JobStoreError>;

    /// Best-effort sweep of terminal jobs past their usefulness window.
    /// Returns the number of evicted jobs.
    async fn sweep(&self, now: DateTime<Utc>) -> Result<u64, JobStoreError>;

    /// Subscribe to terminal events. Every transition to Done, Failed,
    /// or Cancelled fans out through this channel.
    fn subscribe_terminal(&self) -> tokio::sync::broadcast::Receiver<JobTerminalEvent>;
}

#[derive(Debug, thiserror::Error)]
pub enum JobStoreError {
    #[error("job not found: {0}")]
    NotFound(JobId),
    #[error("invalid transition: {0}")]
    InvalidTransition(String),
    #[error("storage error: {0}")]
    Storage(String),
}

// ── JobSink ───────────────────────────────────────────────────

/// A typed handle a provider uses to publish progress and terminal
/// state to the job store.
pub struct JobSink {
    job_id: JobId,
    store: Arc<dyn JobStore>,
}

impl JobSink {
    pub fn new(job_id: JobId, store: Arc<dyn JobStore>) -> Self {
        Self { job_id, store }
    }

    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    pub async fn update_state(&self, state: JobState) -> Result<(), JobStoreError> {
        self.store.update_state(&self.job_id, state).await
    }

    pub async fn update_progress(&self, progress: Progress) -> Result<(), JobStoreError> {
        self.store.update_progress(&self.job_id, progress).await
    }

    pub async fn update_eta(&self, eta: std::time::Duration) -> Result<(), JobStoreError> {
        self.store.update_eta(&self.job_id, eta.as_secs()).await
    }

    pub async fn complete(&self, result: Output) -> Result<(), JobStoreError> {
        self.store.complete(&self.job_id, result).await
    }

    pub async fn fail(&self, error: ProviderError) -> Result<(), JobStoreError> {
        self.store
            .fail(
                &self.job_id,
                serde_json::json!({
                    "code": error.code().as_str(),
                    "message": error.message(),
                }),
            )
            .await
    }
}
