//! `GET /health` — liveness/readiness.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;

pub async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let providers_map = state.capability_directory.providers().await;
    let providers_registered = providers_map.len();
    let providers_enabled = providers_map.values().filter(|p| p.enabled).count();
    Json(json!({
        "status": "ok",
        "directory_version": state.capability_directory.version(),
        "providers_registered": providers_registered,
        "providers_enabled": providers_enabled,
    }))
}
