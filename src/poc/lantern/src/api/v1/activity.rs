//! Activity endpoint — recent garden events from the ring buffer

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::AppState;

/// GET /api/v1/garden/activity — recent events (newest first)
pub async fn get_activity(State(state): State<AppState>) -> Json<Value> {
    let buf = state.activity.read().await;

    // Return events newest-first
    let events: Vec<&crate::infra::event_bus::SseEvent> = buf.iter().rev().collect();

    Json(serde_json::to_value(&events).unwrap())
}
