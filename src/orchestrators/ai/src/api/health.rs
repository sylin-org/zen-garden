//! Health check endpoint.

use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};

/// `GET /health` — returns orchestrator health summary.
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let instances = state.instances.read().await;
    let total = instances.len();
    let healthy = instances.values().filter(|i| i.is_routable()).count();

    Json(serde_json::json!({
        "status": if healthy > 0 { "healthy" } else if total > 0 { "degraded" } else { "no_instances" },
        "offering": "zen-garden.ai.orchestrator",
        "version": env!("CARGO_PKG_VERSION"),
        "instances": { "total": total, "healthy": healthy },
    }))
}
