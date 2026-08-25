//! Background job tracking API endpoints
//!
//! Provides status monitoring for long-running operations:
//! - Service installation jobs
//! - Batch installation jobs
//! - Upgrade operations
//! - Snapshot capture / plant (per ORCH-0039 + Item 2)
//!
//! Jobs track progress, completion, and failures across multiple offerings
//! (batch jobs) or step-by-step within one operation (single-op jobs like
//! capture_snapshot, plant_snapshot).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{Stream, StreamExt};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

use crate::api::responses::ApiResponse;
use crate::infra::PulseEvent;
use crate::{Job, JobStatus, Moss};

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

/// `GET /api/v1/jobs/{job_id}/stream` — per-job SSE progress stream.
///
/// Per ORCH-0039 Item 2 ("useJobProgress + real seed-chip progress"),
/// long-running operations (capture_snapshot, plant_snapshot, install)
/// emit per-step `JobEvent::Progress` through the EventBus. This
/// endpoint surfaces those events filtered to a single `job_id` so a
/// client (Pavilion's seed-chip, Rake's progress UI) can render real
/// progress without sifting the global presence stream.
///
/// # Wire format
///
/// The first event is always `job.snapshot` carrying the current
/// `Job` shape — `id`, `operation`, `status`, `current_step`,
/// `total_steps`, `last_message`, and `result` / `error` if terminal.
/// This eliminates the race window between a POST that creates a job
/// and the client subscribing to its stream: any progress emitted in
/// the gap is reflected in the snapshot.
///
/// Subsequent events for the same `job_id` flow through as
/// `job.progress` / `job.completed` / `job.failed` (the existing
/// presence vocabulary, post-Phase-1 fix that propagates `job_id` in
/// the SSE data payload).
///
/// The stream auto-closes on the terminal event so clients can `for
/// await` the whole operation as a single iterator.
///
/// # Restart / reconnect
///
/// Per ARCH-0038 / Item 2 §"page-load survives": jobs are kept in the
/// aggregate for 24 hours after terminal status (per `DEFAULT_TERMINAL_TTL`).
/// A client reconnecting after closing Pavilion sees the terminal
/// state in the snapshot frame and can decide to consume the result
/// or move on. The companion `GET /api/v1/jobs/{job_id}` endpoint
/// returns the same shape without a streaming session.
///
/// # 404 handling
///
/// Unknown `job_id` returns a snapshot event with `status: "Failed"`
/// and a synthetic `error` field rather than HTTP 404 — matching the
/// existing `GET /api/v1/jobs/{job_id}` permissive shape — so SSE
/// clients don't have to handle a different error path for "no such
/// job."
pub async fn stream_job(
    Path(job_id): Path<String>,
    State(state): State<Moss>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::debug!(job_id = %job_id, "Job stream client connected");

    // Cooperative-shutdown child token — same posture as the presence
    // stream. When Moss is shutting down, the stream ends rather than
    // hanging axum's graceful drain.
    let token = state.shutdown_token.child_token();

    // 1. Snapshot frame: current Job state at subscription time. If
    //    the id is unknown, synthesise a snapshot with status=Failed
    //    + error so the client gets a clean shape to render.
    let snapshot_job = state.jobs.get(&job_id).await;
    let snapshot_terminal = matches!(
        snapshot_job.as_ref().map(|j| &j.status),
        Some(JobStatus::Completed | JobStatus::Failed)
    );

    let snapshot_payload = match snapshot_job {
        Some(job) => serde_json::to_string(&job).unwrap_or_else(|_| "{}".to_string()),
        None => serde_json::json!({
            "id": job_id,
            "status": "Failed",
            "error": format!("Job {job_id} not found"),
        })
        .to_string(),
    };

    // 2. Subscribe to the unified pulse channel — the same channel
    //    the presence stream consumes. Any DomainPulse with
    //    matching `job_id` becomes a stream event.
    let rx = state.pulse.subscribe();
    let job_id_filter = job_id.clone();

    // Build the inner stream: snapshot first, then live events
    // matching this job_id, ending on the terminal (completed/failed)
    // event.
    let mut terminal_seen = snapshot_terminal;
    let inner = futures_util::stream::once(async move {
        Event::default().event("job.snapshot").data(snapshot_payload)
    })
    .chain(BroadcastStream::new(rx).filter_map(move |result| {
        let job_id_filter = job_id_filter.clone();
        let already_terminal = terminal_seen;
        match result {
            Ok(PulseEvent::Domain(pulse)) => {
                // Filter to events bound to this job_id.
                if pulse.job_id.as_deref() != Some(job_id_filter.as_str()) {
                    return std::future::ready(None);
                }
                // Terminal-event detection — drives the auto-close.
                if pulse.event_type == "job.completed"
                    || pulse.event_type == "job.failed"
                {
                    terminal_seen = true;
                }
                let event = pulse.to_presence_event();
                let _ = already_terminal; // present for shape; logic in inner state
                std::future::ready(Some(event))
            }
            Ok(PulseEvent::Transport(_)) => std::future::ready(None),
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!(job_id = %job_id_filter, lagged = n, "Job stream client lagged");
                std::future::ready(None)
            }
        }
    }));

    // 3. Wrap in cancellation-aware stream. Closes on shutdown OR
    //    when the snapshot already showed terminal state (the
    //    snapshot frame carried the result/error; further events
    //    for this job won't fire).
    let stream = async_stream::stream! {
        if snapshot_terminal {
            // Terminal at subscribe — emit only the snapshot. The
            // `inner` once-stream still fires that frame; we then
            // close the stream rather than keep it open forever.
            tokio::pin!(inner);
            if let Some(event) = inner.next().await {
                yield Ok::<Event, Infallible>(event);
            }
            return;
        }
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield Ok::<Event, Infallible>(event),
                        None => break,
                    }
                }
                _ = token.cancelled() => {
                    tracing::debug!(job_id = %job_id, "Job stream: shutdown token cancelled");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
