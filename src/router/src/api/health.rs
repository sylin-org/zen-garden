//! Health check endpoint for container orchestration.

use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

use crate::app_state::AppState;

/// `GET /health` — returns 200 if the router is operational.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let instances = state.instances.read().await;
    let healthy_count = instances
        .values()
        .filter(|i| i.health.is_routable())
        .count();
    let total_count = instances.len();

    Json(json!({
        "status": if healthy_count > 0 { "healthy" } else { "degraded" },
        "instances": {
            "total": total_count,
            "healthy": healthy_count,
        },
        "uptime_secs": state.start_time.elapsed().as_secs(),
    }))
}
