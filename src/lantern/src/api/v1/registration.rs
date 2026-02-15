//! Stone registration endpoint (Moss heartbeat)

use axum::extract::State;
use axum::Json;
use garden_common::RegisterRequest;
use serde_json::{json, Value};

use crate::domain::registration::register_stone;
use crate::AppState;

/// POST /api/v1/register
pub async fn post_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Json<Value> {
    let event = {
        let mut topology = state.topology.write().await;
        register_stone(
            &mut topology,
            req.stone_id.as_deref(),
            &req.stone_name,
            &garden_common::PeerAddress::from_http_url(&req.endpoint),
            req.services,
        )
    };

    tracing::info!(
        stone_name = %req.stone_name,
        event_type = %event.event_type(),
        "Stone registration processed"
    );

    state.event_bus.emit(event);

    Json(json!({
        "ttl_seconds": 135,
        "next_heartbeat_seconds": 45
    }))
}
