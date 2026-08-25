//! Jobs HTTP handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::ids::JobId;
use crate::domain::jobs::{Job, JobCategory, JobFilter, JobState, Progress};

#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    pub category: Option<String>,
    pub state: Option<String>,
    pub owner: Option<String>,
    pub action: Option<String>,
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Response {
    let filter = JobFilter {
        category: q.category.as_ref().and_then(|c| match c.as_str() {
            "api" => Some(JobCategory::Api),
            "provider" => Some(JobCategory::Provider),
            "background" => Some(JobCategory::Background),
            _ => None,
        }),
        state: q.state.as_ref().and_then(|s| match s.as_str() {
            "queued" => Some(JobState::Queued),
            "running" => Some(JobState::Running),
            "done" => Some(JobState::Done),
            "failed" => Some(JobState::Failed),
            "cancelled" => Some(JobState::Cancelled),
            _ => None,
        }),
        owner: q.owner.map(Into::into),
        action_dotted: q.action,
    };
    match state.job_store.list(filter).await {
        Ok(jobs) => {
            let views: Vec<JobView> = jobs.iter().map(JobView::from).collect();
            Json(json!({ "jobs": views })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "internal_error", "message": e.to_string()}})),
        )
            .into_response(),
    }
}

pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = JobId::from_string(id);
    match state.job_store.get(&job_id).await {
        Ok(Some(job)) => Json(JobView::from(&job)).into_response(),
        Ok(None) => (
            StatusCode::GONE,
            Json(json!({"error": {"code": "not_found", "message": "job not found or evicted"}})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "internal_error", "message": e.to_string()}})),
        )
            .into_response(),
    }
}

pub async fn get_job_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = JobId::from_string(id);
    match state.job_store.get(&job_id).await {
        Ok(Some(job)) => {
            if !job.state.is_terminal() {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": {"code": "not_found", "message": "job is not yet terminal"},
                    })),
                )
                    .into_response();
            }
            match job.result {
                Some(out) => Json(json!({ "output": out.to_nested() })).into_response(),
                None => Json(json!({
                    "error": job.error.unwrap_or(json!(null)),
                }))
                .into_response(),
            }
        }
        Ok(None) => (
            StatusCode::GONE,
            Json(json!({"error": {"code": "not_found", "message": "job not found or evicted"}})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "internal_error", "message": e.to_string()}})),
        )
            .into_response(),
    }
}

pub async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = JobId::from_string(id);
    match state.job_store.cancel(&job_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "internal_error", "message": e.to_string()}})),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct JobView {
    pub id: String,
    pub correlation_id: String,
    pub category: JobCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub state: JobState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: Value,
}

impl From<&Job> for JobView {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id.as_str().to_string(),
            correlation_id: job.correlation_id.as_str().to_string(),
            category: job.category,
            owner: job.owner.as_ref().map(|p| p.as_str().to_string()),
            action: job.action.as_ref().map(|a| a.dotted()),
            state: job.state,
            progress: job.progress.clone(),
            eta_seconds: job.eta_seconds,
            created_at: job.created_at,
            updated_at: job.updated_at,
            terminal_at: job.terminal_at,
            metadata: job.metadata.clone(),
        }
    }
}
