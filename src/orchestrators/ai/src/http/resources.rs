//! `GET /v1/resources` and `GET /v1/resources/stones/{name}` —
//! REST surface for the Resources domain (ORCH-0030 §2).
//!
//! These are the authoritative state queries that complement the
//! `resources.stone.{name}.*` events on the unified bus. Per the
//! ADR's REST/SSE separation: REST carries state, the bus carries
//! transitions.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;
use crate::domain::resources::StoneName;

pub async fn list_resources(State(state): State<AppState>) -> Response {
    let stones = state.resources.snapshot_all().await;
    let body = json!({
        "count": stones.len(),
        "stones": stones,
    });
    (StatusCode::OK, Json(body)).into_response()
}

pub async fn get_stone_resources(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let sn = StoneName::new(name.clone());
    match state.resources.snapshot(&sn).await {
        Some(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "not_found",
                    "message": format!("no resources tracked for stone `{name}`")
                }
            })),
        )
            .into_response(),
    }
}

pub async fn get_stone_pressure(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let sn = StoneName::new(name.clone());
    match state.resources.pressure(&sn).await {
        Some(p) => (StatusCode::OK, Json(p)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "not_found",
                    "message": format!("no resources tracked for stone `{name}`")
                }
            })),
        )
            .into_response(),
    }
}

/// `GET /v1/resources/tiers` — observability endpoint that
/// buckets garden stones by largest-GPU VRAM capacity (4, 8, 12,
/// 16, 24, 32, 48, 80, 96 GB plus a frontier bucket above 96).
/// Feeds dashboards that want to render "resource pressure by
/// tier" views or capacity-planning prompts like "your garden has
/// no stones above the 16 GB tier — certain models will never be
/// available".
///
/// Stones with no GPUs (or GPUs whose total VRAM is unknown)
/// land in a bucket with `max_vram_gb: 0`.
pub async fn get_tier_summary(State(state): State<AppState>) -> Response {
    let tiers = state.resources.tier_summary().await;
    let body = json!({
        "tiers": tiers,
    });
    (StatusCode::OK, Json(body)).into_response()
}
