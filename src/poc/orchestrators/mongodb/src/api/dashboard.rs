//! Dashboard API and SSE event stream.

use crate::app_state::AppState;
use axum::extract::State;
use axum::response::Html;
use axum::response::sse::Sse;
use axum::Json;
use serde_json::{json, Value};

/// Embedded dashboard HTML (compiled into the binary).
const DASHBOARD_HTML: &str = include_str!("../../assets/dashboard.html");

/// `GET /` — serve the dashboard HTML page.
pub async fn get_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

/// `GET /api/status` — full cluster status JSON.
pub async fn get_status(State(state): State<AppState>) -> Json<Value> {
    let instances = state.instances.read().await;
    let replica_sets = state.replica_sets.read().await;
    let tended = state.tended_stone.read().await;

    let uptime = state.start_time.elapsed().as_secs();

    let instance_list: Vec<Value> = instances
        .values()
        .map(|i| {
            json!({
                "stone_id": i.stone_id,
                "stone_name": i.stone_name,
                "mongo_endpoint": i.mongo_endpoint,
                "moss_endpoint": i.moss_endpoint,
                "fqn": i.fqn,
                "health": i.health,
                "role": i.role,
            })
        })
        .collect();

    let rs_list: Vec<Value> = replica_sets
        .values()
        .map(|rs| {
            json!({
                "rs_name": rs.rs_name,
                "initialized": rs.initialized,
                "members": rs.members.len(),
                "connection_string": rs.connection_string,
                "last_updated": rs.last_updated.to_rfc3339(),
            })
        })
        .collect();

    Json(json!({
        "offering": state.offering_name,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime,
        "tended_stone": tended.as_ref().map(|s| json!({
            "stone_name": s.stone_name,
            "endpoint": s.endpoint,
        })),
        "instances": instance_list,
        "replica_sets": rs_list,
    }))
}

/// `GET /api/events` — SSE stream for dashboard updates.
pub async fn get_events(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>
{
    orchestrator_common::events::dashboard_sse_stream(&state.dashboard_tx)
}
