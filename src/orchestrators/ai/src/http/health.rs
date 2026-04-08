//! `GET /health` — liveness/readiness.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;

pub async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.directory.snapshot();
    Json(json!({
        "status": "ok",
        "directory_version": snapshot.version,
        "providers_registered": snapshot.providers_count(),
        "providers_healthy": snapshot.healthy_provider_count(),
    }))
}
