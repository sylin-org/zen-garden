//! Operator endpoints for capability-keyed recommendation pins.
//!
//! URL grammar:
//!
//! - `GET  /v1/recommendations` — full ranked cache, keyed by
//!   capability label.
//! - `GET  /v1/recommendations/{capability}` — one capability's
//!   ranked list with reasoning breadcrumbs.
//! - `PUT  /v1/recommendations/{capability}` body
//!   `{"model": "<provider>|<short>"}` — pin a model.
//! - `DELETE /v1/recommendations/{capability}` — remove the pin.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::app_state::AppState;
use crate::domain::ids::ModelFqn;
use crate::domain::recommendation_types::Pin;

pub async fn list_recommendations(State(state): State<AppState>) -> Response {
    let cache = state.recommendation.snapshot();
    Json(json!({
        "version": cache.version,
        "built_at": cache.built_at,
        "per_capability": cache.per_capability,
    }))
    .into_response()
}

pub async fn get_recommendation(
    State(state): State<AppState>,
    Path(capability): Path<String>,
) -> Response {
    let cache = state.recommendation.snapshot();
    match cache.per_capability.get(&capability) {
        Some(ranked) => Json(ranked).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "not_found",
                    "message": format!("unknown capability `{capability}`"),
                },
            })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct PinBody {
    pub model: String,
    pub note: Option<String>,
    pub pinned_by: Option<String>,
}

pub async fn put_recommendation(
    State(state): State<AppState>,
    Path(capability): Path<String>,
    Json(body): Json<PinBody>,
) -> Response {
    // Reject pins for unknown capabilities up front so the
    // operator gets a clear 404 instead of a silently-ignored pin.
    if state
        .recommendation
        .profiles()
        .get(&capability)
        .is_none()
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "not_found",
                    "message": format!("unknown capability `{capability}`"),
                },
            })),
        )
            .into_response();
    }

    let model = match ModelFqn::parse(&body.model) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {"code": "validation_failed", "message": format!("model FQN: {e}")},
                })),
            )
                .into_response();
        }
    };
    let pin = Pin {
        capability: capability.clone(),
        model,
        pinned_at: Utc::now(),
        pinned_by: body.pinned_by,
        note: body.note,
    };
    if let Err(e) = state.recommendation.pins().set(pin.clone()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {"code": "internal_error", "message": e.to_string()},
            })),
        )
            .into_response();
    }
    state.recommendation.rebuild().await;
    Json(pin).into_response()
}

pub async fn delete_recommendation(
    State(state): State<AppState>,
    Path(capability): Path<String>,
) -> Response {
    if state
        .recommendation
        .profiles()
        .get(&capability)
        .is_none()
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "not_found",
                    "message": format!("unknown capability `{capability}`"),
                },
            })),
        )
            .into_response();
    }
    if let Err(e) = state.recommendation.pins().delete(&capability).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {"code": "internal_error", "message": e.to_string()},
            })),
        )
            .into_response();
    }
    state.recommendation.rebuild().await;
    StatusCode::NO_CONTENT.into_response()
}
