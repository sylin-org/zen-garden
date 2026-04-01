//! Health check endpoint.

use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};

/// `GET /health` — returns orchestrator health summary.
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let snap = state.registry.snapshot().clone();
    let total = snap.instances.len();
    let healthy = snap.instances.values().filter(|i| i.is_routable()).count();

    Json(serde_json::json!({
        "status": if healthy > 0 { "healthy" } else if total > 0 { "degraded" } else { "no_instances" },
        "offering": "zen-garden.ai.orchestrator",
        "version": env!("CARGO_PKG_VERSION"),
        "instances": { "total": total, "healthy": healthy },
    }))
}
