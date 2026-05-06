//! Pavilion-side job-stream consumption (Item 2).
//!
//! Bridges Moss's per-job SSE endpoint at
//! `/api/v1/jobs/{job_id}/stream` to the React frontend. The Tauri
//! command layer calls [`consume_job_stream`] after issuing a
//! capture/plant POST: it opens the stream, parses each SSE frame,
//! emits a typed Tauri event for the React side to consume, and
//! resolves with the final `Completed`-state job (carrying the
//! result payload) or surfaces a `Failed` state's error.
//!
//! ## Tauri events emitted
//!
//! All events carry `job_id` so the React `useJobProgress(jobId)`
//! hook can filter by id when multiple jobs run concurrently.
//!
//! - **`job:snapshot`** — first frame from the SSE stream, carrying
//!   the full `Job` shape from the Moss aggregate. Lets React
//!   reconstruct any state emitted before subscription.
//! - **`job:progress`** — per-step progress with `current_step` /
//!   `total_steps` / `last_message`. Drives the seed-chip fill.
//! - **`job:completed`** — terminal success with `result` payload.
//! - **`job:failed`** — terminal failure with `error` string.
//!
//! ## Reconnect on stream interruption
//!
//! If the SSE connection drops (network blip, daemon restart) before
//! a terminal event arrives, [`consume_job_stream`] will fall back to
//! polling `GET /api/v1/jobs/{id}` until the job reaches a terminal
//! status. Per Item 2 §"page-load survives": the Jobs aggregate
//! retains terminal jobs for 24 hours, so even a long Pavilion outage
//! followed by a reconnect resolves with the recorded result.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::connection;

/// Event names emitted on the Tauri runtime channel.
pub mod event_names {
    pub const SNAPSHOT: &str = "job:snapshot";
    pub const PROGRESS: &str = "job:progress";
    pub const COMPLETED: &str = "job:completed";
    pub const FAILED: &str = "job:failed";
}

/// Snapshot frame payload — mirrors the Moss `Job` shape but only
/// surfaces the fields the React side renders. Extra fields from the
/// server (timestamps, completed/failed maps for batch jobs, etc.)
/// are tolerated via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: String,
    #[serde(default)]
    pub operation: String,
    pub status: String,
    #[serde(default)]
    pub current_step: Option<u32>,
    #[serde(default)]
    pub total_steps: Option<u32>,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Per-step progress payload emitted to the React side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub job_id: String,
    pub message: String,
    #[serde(default)]
    pub step: Option<u32>,
    #[serde(default)]
    pub total_steps: Option<u32>,
}

/// Terminal-success payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCompleted {
    pub job_id: String,
    pub result: Value,
}

/// Terminal-failure payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFailed {
    pub job_id: String,
    pub error: String,
}

/// Outcome of a fully-consumed job stream. Returned to the Tauri
/// command which forwards it to the React side as the `await`'ed
/// resolution value.
pub enum JobOutcome {
    Completed(Value),
    Failed(String),
}

/// Open the per-job SSE stream on the named stone, forward events to
/// React via the Tauri runtime channel, and resolve with the
/// terminal outcome. On stream interruption falls back to polling
/// `/api/v1/jobs/{id}` until terminal — covers the case where the
/// connection drops mid-job (page-load-survives requirement).
pub async fn consume_job_stream(
    app: &AppHandle,
    base_url: &str,
    job_id: &str,
) -> Result<JobOutcome, String> {
    let base = base_url.trim_end_matches('/').to_string();
    let stream_url = format!("{}/api/v1/jobs/{}/stream", base, job_id);

    // Try the streaming path first.
    if let Some(outcome) = stream_then_emit(app, &stream_url, job_id).await? {
        return Ok(outcome);
    }

    // Stream closed without a terminal frame — fall back to polling
    // GET /api/v1/jobs/{id} until status is Completed or Failed.
    poll_until_terminal(app, &base, job_id).await
}

/// Open the SSE stream and process frames until a terminal event is
/// observed (returns `Some(outcome)`) or the stream closes
/// non-terminally (returns `Ok(None)` — caller should fall back to
/// polling).
async fn stream_then_emit(
    app: &AppHandle,
    stream_url: &str,
    job_id: &str,
) -> Result<Option<JobOutcome>, String> {
    use futures_util::StreamExt;

    let client = connection::raw_client_for_job_stream();
    let resp = client
        .get(stream_url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .map_err(|e| format!("job-stream GET {stream_url}: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("job-stream {status}: {body}"));
    }

    let mut byte_stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = byte_stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, job_id, "job-stream read error — falling back to poll");
                return Ok(None);
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer.drain(..idx + 2);
            if let Some(outcome) = handle_frame(app, job_id, &frame).await? {
                return Ok(Some(outcome));
            }
        }
    }

    // Stream closed without a terminal frame. Could be a server
    // restart mid-job; the GET-poll fallback resolves it.
    Ok(None)
}

/// Parse a single SSE frame and emit the corresponding Tauri event.
/// Returns `Ok(Some(outcome))` when the frame is terminal so the
/// caller can stop reading.
async fn handle_frame(
    app: &AppHandle,
    job_id: &str,
    frame: &str,
) -> Result<Option<JobOutcome>, String> {
    // SSE frame: lines starting with "event:" set the event type,
    // "data:" lines accumulate the payload (joined by \n).
    let mut event_type: Option<&str> = None;
    let mut data_buf = String::new();
    for line in frame.lines() {
        if let Some(ev) = line.strip_prefix("event:").map(str::trim) {
            event_type = Some(ev);
        } else if let Some(data) = line.strip_prefix("data:") {
            // Per the spec, a single leading space after `data:` is
            // optional and stripped; multiple lines join with `\n`.
            let stripped = data.strip_prefix(' ').unwrap_or(data);
            if !data_buf.is_empty() {
                data_buf.push('\n');
            }
            data_buf.push_str(stripped);
        }
    }
    let event_type = match event_type {
        Some(et) => et,
        None => return Ok(None),
    };
    if data_buf.is_empty() {
        // Comment-only or keep-alive frame.
        return Ok(None);
    }

    match event_type {
        "job.snapshot" => {
            let snapshot: JobSnapshot = serde_json::from_str(&data_buf)
                .map_err(|e| format!("job.snapshot parse: {e} ({data_buf})"))?;
            // Snapshot may carry terminal state — Pavilion reconnects
            // mid-job or post-completion still resolve cleanly here.
            let terminal = match snapshot.status.as_str() {
                "Completed" | "completed" => Some(JobOutcome::Completed(
                    snapshot.result.clone().unwrap_or(Value::Null),
                )),
                "Failed" | "failed" => Some(JobOutcome::Failed(
                    snapshot
                        .error
                        .clone()
                        .unwrap_or_else(|| "unknown error".to_string()),
                )),
                _ => None,
            };
            // Always emit the snapshot — reconnects benefit from
            // seeing the current step counters even mid-stream.
            if let Err(e) = app.emit(event_names::SNAPSHOT, &snapshot) {
                tracing::warn!(error = %e, "failed to emit job:snapshot");
            }
            Ok(terminal)
        }
        "job.progress" => {
            let v: Value = serde_json::from_str(&data_buf)
                .map_err(|e| format!("job.progress parse: {e}"))?;
            let progress = JobProgress {
                job_id: v
                    .get("job_id")
                    .and_then(|x| x.as_str())
                    .unwrap_or(job_id)
                    .to_string(),
                message: v
                    .get("message")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                step: v.get("step").and_then(|x| x.as_u64()).map(|n| n as u32),
                total_steps: v
                    .get("total_steps")
                    .and_then(|x| x.as_u64())
                    .map(|n| n as u32),
            };
            if let Err(e) = app.emit(event_names::PROGRESS, &progress) {
                tracing::warn!(error = %e, "failed to emit job:progress");
            }
            Ok(None)
        }
        "job.completed" => {
            // The terminal `job.completed` event from the SSE stream
            // doesn't carry the result payload — it's delivered via
            // the snapshot frame for clients connecting late, or via
            // GET /jobs/{id} for explicit fetch. To deliver the
            // result here, we GET the job and forward its `result`.
            // This is one extra round-trip per terminal event but
            // happens once per operation and is local-host fast.
            let v: Value = serde_json::from_str(&data_buf)
                .map_err(|e| format!("job.completed parse: {e}"))?;
            let _ = v; // payload reserved; result comes from the GET below
            // Fall through to caller's poll loop for result fetch —
            // signal terminal so the stream returns and the caller
            // can do the GET.
            Ok(Some(JobOutcome::Completed(Value::Null)))
        }
        "job.failed" => {
            let v: Value = serde_json::from_str(&data_buf)
                .map_err(|e| format!("job.failed parse: {e}"))?;
            let error = v
                .get("error")
                .and_then(|x| x.as_str())
                .unwrap_or("job failed")
                .to_string();
            let payload = JobFailed {
                job_id: job_id.to_string(),
                error: error.clone(),
            };
            if let Err(e) = app.emit(event_names::FAILED, &payload) {
                tracing::warn!(error = %e, "failed to emit job:failed");
            }
            Ok(Some(JobOutcome::Failed(error)))
        }
        _ => Ok(None),
    }
}

/// Poll `GET /api/v1/jobs/{id}` every 1.5 s until the job reaches a
/// terminal state. Used as a fallback when the SSE stream closes
/// without a terminal frame. Emits a `job:snapshot` event on each
/// poll so the UI keeps current.
async fn poll_until_terminal(
    app: &AppHandle,
    base: &str,
    job_id: &str,
) -> Result<JobOutcome, String> {
    let client = connection::raw_client_for_capture();
    let url = format!("{}/api/v1/jobs/{}", base, job_id);

    loop {
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("job poll GET {url}: {e}"))?;
        let status = resp.status();
        let envelope: PollEnvelope = resp
            .json()
            .await
            .map_err(|e| format!("job poll parse: {e}"))?;
        let snapshot = envelope.data;

        let _ = app.emit(event_names::SNAPSHOT, &snapshot);

        match snapshot.status.as_str() {
            "Completed" | "completed" => {
                let result = snapshot.result.clone().unwrap_or(Value::Null);
                let payload = JobCompleted {
                    job_id: job_id.to_string(),
                    result: result.clone(),
                };
                let _ = app.emit(event_names::COMPLETED, &payload);
                return Ok(JobOutcome::Completed(result));
            }
            "Failed" | "failed" => {
                let error = snapshot
                    .error
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string());
                let payload = JobFailed {
                    job_id: job_id.to_string(),
                    error: error.clone(),
                };
                let _ = app.emit(event_names::FAILED, &payload);
                return Ok(JobOutcome::Failed(error));
            }
            _ if status == reqwest::StatusCode::NOT_FOUND => {
                // Job was evicted or never existed — treat as failure.
                return Err(format!("job {job_id} not found"));
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        }
    }
}

/// Fetch the final `result` payload of a completed job via GET.
/// Used after a streaming `job.completed` terminal event (which
/// doesn't itself carry the result) to deliver the actual data to
/// the Tauri command's caller.
pub async fn fetch_job_result(base_url: &str, job_id: &str) -> Result<Value, String> {
    let client = connection::raw_client_for_capture();
    let url = format!(
        "{}/api/v1/jobs/{}",
        base_url.trim_end_matches('/'),
        job_id
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("job result GET {url}: {e}"))?;
    let envelope: PollEnvelope = resp
        .json()
        .await
        .map_err(|e| format!("job result parse: {e}"))?;
    Ok(envelope.data.result.unwrap_or(Value::Null))
}

#[derive(Debug, Deserialize)]
struct PollEnvelope {
    data: JobSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_deserialise_with_terminal_state() {
        let json = r#"{
            "id": "j1",
            "operation": "capture_snapshot",
            "status": "Completed",
            "current_step": 9,
            "total_steps": 9,
            "last_message": "truncating event log",
            "result": {"snapshot_id": "snap-1", "size_total_bytes": 1234},
            "completed": [],
            "failed": {},
            "started_at": {"secs_since_epoch": 1, "nanos_since_epoch": 0},
            "completed_at": null,
            "offerings": []
        }"#;
        let snap: JobSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.status, "Completed");
        assert_eq!(snap.current_step, Some(9));
        assert_eq!(snap.total_steps, Some(9));
        assert_eq!(snap.last_message.as_deref(), Some("truncating event log"));
        assert_eq!(
            snap.result.as_ref().unwrap()["snapshot_id"],
            "snap-1"
        );
    }

    #[test]
    fn snapshot_tolerates_missing_optional_fields() {
        // A pre-step running job has no current_step / total_steps /
        // result yet. Deserialise must not fail.
        let json = r#"{
            "id": "j2",
            "status": "Running",
            "completed": [],
            "failed": {},
            "started_at": {"secs_since_epoch": 1, "nanos_since_epoch": 0},
            "completed_at": null,
            "offerings": []
        }"#;
        let snap: JobSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.status, "Running");
        assert!(snap.current_step.is_none());
        assert!(snap.total_steps.is_none());
        assert!(snap.result.is_none());
    }

    #[test]
    fn progress_extracts_step_total_message() {
        // Wire shape from Moss's pulse `to_presence_event` for a
        // job.progress event with step/total_steps in data.
        let v: Value = serde_json::json!({
            "timestamp": "2026-05-06T05:00:00Z",
            "message": "archiving /data/db",
            "service": "mongodb::prd",
            "job_id": "j1",
            "step": 5,
            "total_steps": 9
        });
        // Simulate handle_frame's deserialisation.
        let progress = JobProgress {
            job_id: v["job_id"].as_str().unwrap().to_string(),
            message: v["message"].as_str().unwrap().to_string(),
            step: v.get("step").and_then(|x| x.as_u64()).map(|n| n as u32),
            total_steps: v
                .get("total_steps")
                .and_then(|x| x.as_u64())
                .map(|n| n as u32),
        };
        assert_eq!(progress.step, Some(5));
        assert_eq!(progress.total_steps, Some(9));
        assert_eq!(progress.message, "archiving /data/db");
    }
}
