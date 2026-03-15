//! Health check endpoint

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// GET /health
pub async fn get_health(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;
    let stones_online = topology.stones_online_count();
    let stones_total = topology.stones_total_count();
    let uptime_secs = state.start_time.elapsed().as_secs();

    Json(json!({
        "status": "healthy",
        "lantern_name": state.name,
        "port": state.api_port,
        "stones_online": stones_online,
        "stones_total": stones_total,
        "uptime_seconds": uptime_secs,
    }))
}
