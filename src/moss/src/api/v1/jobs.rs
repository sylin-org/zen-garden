//! Background job tracking API endpoints
//!
//! Provides status monitoring for long-running operations:
//! - Service installation jobs
//! - Batch installation jobs
//! - Upgrade operations
//!
//! Jobs track progress, completion, and failures across multiple offerings.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::collections::HashMap;

use crate::api::responses::ApiResponse;
use crate::{Moss, Job, JobStatus};

/// GET /api/v1/jobs/:job_id - Get status of a specific job
///
/// Returns the current status of a background job including:
/// - Job state (Pending, Running, Completed, Failed)
/// - List of completed offerings
/// - Map of failed offerings with error messages
/// - Start and completion timestamps
///
/// # Returns
/// - 200 OK: Job found, returns job details
/// - 404 NOT FOUND: Job ID doesn't exist (returns stub job with suggestion)
///
/// # Example Response
/// ```json
/// {
///   "data": {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "offerings": ["nginx", "redis", "postgres"],
///     "status": "Running",
///     "completed": ["nginx", "redis"],
///     "failed": {},
///     "started_at": "2026-01-21T12:00:00Z",
///     "completed_at": null
///   }
/// }
/// ```
pub async fn get_job_status(
    Path(job_id): Path<String>,
    State(state): State<Moss>,
) -> (StatusCode, Json<ApiResponse<Job>>) {
    match state.jobs.get(&job_id).await {
        Some(job) => (StatusCode::OK, Json(ApiResponse::new(job))),
        None => {
            // Job not found - return 404 with stub and helpful suggestion
            (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::with_suggestions(
                    Job {
                        id: job_id.clone(),
                        operation: String::new(),
                        status: JobStatus::Failed,
                        targets: vec![],
                        completed: vec![],
                        failed: HashMap::new(),
                        started_at: std::time::SystemTime::now(),
                        completed_at: Some(std::time::SystemTime::now()),
                        current_step: None,
                        total_steps: None,
                        last_message: None,
                        result: None,
                        error: Some(format!("Job {job_id} not found")),
                    },
                    vec!["Check job ID is correct".to_string()],
                )),
            )
        }
    }
}

/// GET /api/v1/jobs - List all background jobs
///
/// Returns all jobs currently tracked in the system.
/// Jobs are kept in memory and lost on daemon restart.
///
/// # Returns
/// - 200 OK: Array of all jobs (may be empty)
///
/// # Example Response
/// ```json
/// {
///   "data": [
///     {
///       "id": "550e8400-e29b-41d4-a716-446655440000",
///       "offerings": ["nginx"],
///       "status": "Completed",
///       "completed": ["nginx"],
///       "failed": {},
///       "started_at": "2026-01-21T12:00:00Z",
///       "completed_at": "2026-01-21T12:00:30Z"
///     }
///   ]
/// }
/// ```
pub async fn list_jobs(State(state): State<Moss>) -> (StatusCode, Json<ApiResponse<Vec<Job>>>) {
    let job_list = state.jobs.snapshot().await;
    (StatusCode::OK, Json(ApiResponse::new(job_list)))
}
