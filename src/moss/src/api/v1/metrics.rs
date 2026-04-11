//! Metrics aggregate API endpoints (ARCH-0018)
//!
//! HTTP read surface for the Metrics bounded context. Seven handlers
//! plus one SSE stream:
//!
//! | Path | Returns |
//! |------|---------|
//! | `GET /api/v1/stone/metrics`                    | Full snapshot (global + domains + tasks) |
//! | `GET /api/v1/stone/metrics/global`             | Process-wide counters only |
//! | `GET /api/v1/stone/metrics/domains`            | All domain observability |
//! | `GET /api/v1/stone/metrics/domains/{name}`     | One domain (404 if unknown) |
//! | `GET /api/v1/stone/metrics/tasks`              | All task observability |
//! | `GET /api/v1/stone/metrics/tasks/{name}`       | One task (404 if unknown) |
//! | `GET /api/v1/stone/metrics/stream`             | SSE of `MetricsChanged` events |
//!
//! Every handler is a thin dispatcher that calls into `Arc<Metrics>`
//! via `FromRef` extraction, matching the chapter template's
//! command/query separation principle: handlers never touch state
//! except through typed methods on the aggregate.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::AppState;
use crate::api::responses::ApiResponse;
use crate::domain::Metrics;
use crate::domain::metrics::{DomainSnapshot, GlobalSnapshot, MetricsSnapshot, TaskSnapshot};

// ─── Snapshot endpoints ───────────────────────────────────────────────

/// `GET /api/v1/stone/metrics` — full observability snapshot.
///
/// Returns global counters, every registered domain's metrics, and
/// every registered task's metrics in one JSON object. Consumers that
/// want to poll regularly should prefer the sub-path endpoints for
/// smaller responses.
pub async fn get_metrics(
    State(metrics): State<Arc<Metrics>>,
) -> Json<ApiResponse<MetricsSnapshot>> {
    Json(ApiResponse::new(metrics.snapshot().await))
}

/// `GET /api/v1/stone/metrics/global` — process-wide counters only.
pub async fn get_metrics_global(
    State(metrics): State<Arc<Metrics>>,
) -> Json<ApiResponse<GlobalSnapshot>> {
    Json(ApiResponse::new(metrics.global().await))
}

/// `GET /api/v1/stone/metrics/domains` — all registered domains.
pub async fn get_metrics_domains(
    State(metrics): State<Arc<Metrics>>,
) -> Json<ApiResponse<Vec<DomainSnapshot>>> {
    Json(ApiResponse::new(metrics.domains().await))
}

/// `GET /api/v1/stone/metrics/domains/{name}` — single domain.
///
/// Returns 404 Not Found if the domain is not registered.
pub async fn get_metrics_domain(
    State(metrics): State<Arc<Metrics>>,
    Path(name): Path<String>,
) -> Response {
    match metrics.domain(&name).await {
        Some(snapshot) => Json(ApiResponse::new(snapshot)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "DOMAIN_NOT_FOUND",
                "message": format!("Metrics domain '{}' is not registered", name),
            })),
        )
            .into_response(),
    }
}

/// `GET /api/v1/stone/metrics/tasks` — all registered tasks.
pub async fn get_metrics_tasks(
    State(metrics): State<Arc<Metrics>>,
) -> Json<ApiResponse<Vec<TaskSnapshot>>> {
    Json(ApiResponse::new(metrics.tasks().await))
}

/// `GET /api/v1/stone/metrics/tasks/{name}` — single task.
///
/// Returns 404 Not Found if the task is not registered.
///
/// Note: this endpoint returns **observability data** (timings,
/// event counts, lag) for a task. For **lifecycle state**
/// (Waiting/Running/Completed) see `GET /api/v1/stone/tasks/{name}`,
/// which reads from `SupervisorHandle`. The two endpoints are
/// complementary per ARCH-0018.
pub async fn get_metrics_task(
    State(metrics): State<Arc<Metrics>>,
    Path(name): Path<String>,
) -> Response {
    match metrics.task(&name).await {
        Some(snapshot) => Json(ApiResponse::new(snapshot)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "TASK_NOT_FOUND",
                "message": format!("Task '{}' is not registered with Metrics", name),
            })),
        )
            .into_response(),
    }
}

// ─── SSE stream ────────────────────────────────────────────────────────

/// `GET /api/v1/stone/metrics/stream` — live transition event stream.
///
/// Subscribes to the Metrics aggregate's `changes()` broadcast and
/// emits each `MetricsChanged` event as a Server-Sent Event. Fires
/// only on **interesting transitions** (task state changes, lag
/// detection, domain/task registration) — counter increments do NOT
/// appear on this stream and must be polled via `GET /metrics`.
///
/// The stream is cancellation-aware: it ends cleanly when the moss
/// shutdown token fires (MOSS-0004).
pub async fn stream_metrics(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let token = state.shutdown_token.child_token();
    let rx = state.metrics.changes();

    let inner = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(json) => Some(Event::default().data(json)),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize MetricsChanged for SSE");
                None
            }
        },
        // Lagged or closed — stream observer just misses events.
        // The producer (Metrics) keeps running.
        Err(_) => None,
    });

    let stream = async_stream::stream! {
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield Ok::<Event, Infallible>(event),
                        None => break,
                    }
                }
                _ = token.cancelled() => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
