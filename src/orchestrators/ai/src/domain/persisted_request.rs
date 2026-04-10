//! Persistent request log (ORCH-0033).
//!
//! Extends the ephemeral `OrchestratorRequest` lifecycle: every user
//! interaction is stored as a durable record with input, output, media
//! references, resolution metadata, and fork lineage.
//!
//! Separation of concerns: Jobs track operational lifecycle (queued →
//! running → done). Requests track user interactions (what was asked,
//! what was returned, lineage, bookmarks).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::ids::{JobId, RequestId};

// ── Status ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Running,
    Success,
    Failure,
}

// ── Media reference ──────────────────────────────────────────

/// A media artifact referenced by a request — either as input
/// (provided by the user) or output (produced by the orchestrator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMedia {
    pub media_id: String,
    pub field: String,
    pub content_type: String,
}

// ── Selectors snapshot ───────────────────────────────────────

/// Snapshot of the selectors at dispatch time. Captures what the user
/// requested (which may differ from what was resolved).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectorsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

// ── Resolution metadata ──────────────────────────────────────

/// Metadata captured after dispatch — what actually happened.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
}

// ── Error snapshot ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSnapshot {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

// ── The persisted request ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRequest {
    // Identity
    pub id: String,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    // Lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    // Intent
    pub action: String,
    pub status: RequestStatus,
    pub input: Value,
    #[serde(default)]
    pub selectors: SelectorsSnapshot,

    // Output
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorSnapshot>,

    // Media
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_inputs: Vec<RequestMedia>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_outputs: Vec<RequestMedia>,

    // Resolution metadata
    #[serde(default)]
    pub meta: RequestMeta,

    // Retention
    #[serde(default)]
    pub pinned: bool,

    // Job link (operational, not user-facing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

impl PersistedRequest {
    /// Create a new request record at dispatch time (status: Running).
    pub fn new_running(
        id: RequestId,
        correlation_id: String,
        action: String,
        input: Value,
        selectors: SelectorsSnapshot,
        media_inputs: Vec<RequestMedia>,
        resolved_provider: Option<String>,
        job_id: Option<JobId>,
        parent_id: Option<String>,
    ) -> Self {
        Self {
            id: id.as_str().to_string(),
            correlation_id,
            created_at: Utc::now(),
            completed_at: None,
            parent_id,
            action,
            status: RequestStatus::Running,
            input,
            selectors,
            output: None,
            error: None,
            media_inputs,
            media_outputs: Vec::new(),
            meta: RequestMeta {
                provider: resolved_provider,
                ..Default::default()
            },
            pinned: false,
            job_id: job_id.map(|j| j.as_str().to_string()),
        }
    }

    /// Mark the request as successfully completed.
    pub fn complete(
        &mut self,
        output: Value,
        media_outputs: Vec<RequestMedia>,
        meta: RequestMeta,
    ) {
        self.status = RequestStatus::Success;
        self.completed_at = Some(Utc::now());
        self.output = Some(output);
        self.media_outputs = media_outputs;
        self.meta = meta;
    }

    /// Mark the request as failed.
    pub fn fail(&mut self, error: ErrorSnapshot) {
        self.status = RequestStatus::Failure;
        self.completed_at = Some(Utc::now());
        self.error = Some(error);
    }

    /// All media IDs referenced by this request (inputs + outputs).
    pub fn all_media_ids(&self) -> Vec<&str> {
        self.media_inputs
            .iter()
            .chain(self.media_outputs.iter())
            .map(|m| m.media_id.as_str())
            .collect()
    }
}

// ── Filter for listing ───────────────────────────────────────

#[derive(Debug, Default)]
pub struct RequestFilter {
    pub action: Option<String>,
    pub status: Option<RequestStatus>,
    pub pinned: Option<bool>,
    pub parent_id: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub after: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}
