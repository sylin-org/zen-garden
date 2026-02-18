//! Dashboard: serves the single-page HTML dashboard and its data endpoints.

use crate::app_state::AppState;
use crate::infra::events::dashboard_sse_stream;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};
use serde_json::json;

/// The dashboard HTML — embedded at compile time (same pattern as Moss Portrait).
const DASHBOARD_HTML: &str = include_str!("../../assets/dashboard.html");

/// `GET /` — serve the dashboard SPA.
pub async fn get_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

/// `GET /api/status` — full router status JSON (polled by dashboard).
///
/// Reads from the pre-built snapshot (zero locks on the request path).
/// The snapshot is published every 2s by the snapshot_publisher task.
pub async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.snapshot_rx.borrow().clone())
}

/// `GET /api/events` — SSE stream for real-time dashboard updates.
pub async fn get_events(State(state): State<AppState>) -> impl IntoResponse {
    dashboard_sse_stream(&state.dashboard_tx)
}

/// `GET /api/settings` — current configuration.
pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(json!(*config))
}

/// `POST /api/settings` — update configuration.
pub async fn post_settings(
    State(state): State<AppState>,
    Json(new_config): Json<crate::domain::types::RouterConfig>,
) -> impl IntoResponse {
    {
        let mut config = state.config.write().await;
        *config = new_config.clone();
    }

    // Update metrics engine enabled flag
    {
        let mut metrics = state.metrics.write().await;
        metrics.enabled = new_config.features.metrics_enabled;
    }

    // Persist to disk
    if let Err(e) = crate::infra::persistence::save_config(&state.data_dir, &new_config).await {
        tracing::warn!(error = %e, "failed to persist config");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})));
    }

    state.emit_event("config.updated", "{}").await;
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

/// `POST /api/metrics/reset` — reset all metrics counters and clear persisted data.
pub async fn post_metrics_reset(State(state): State<AppState>) -> impl IntoResponse {
    {
        let mut metrics = state.metrics.write().await;
        metrics.reset();
    }

    // Clear the metrics/ folder on disk
    if let Err(e) = crate::infra::persistence::clear_metrics(&state.data_dir).await {
        tracing::warn!(error = %e, "failed to clear metrics folder");
    }

    state.emit_event("metrics.reset", "{}").await;
    Json(json!({"status": "ok"}))
}

/// `GET /api/jobs` — current and recent jobs.
pub async fn get_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.jobs.read().await;
    let list: Vec<serde_json::Value> = jobs
        .iter()
        .rev()
        .map(|j| {
            json!({
                "id": j.id,
                "kind": j.kind.label(),
                "subject": j.kind.subject(),
                "status": j.status,
                "progress": j.progress,
                "started_at": j.started_at.to_rfc3339(),
                "completed_at": j.completed_at.map(|t| t.to_rfc3339()),
                "error": j.error,
            })
        })
        .collect();
    Json(json!({"jobs": list}))
}
