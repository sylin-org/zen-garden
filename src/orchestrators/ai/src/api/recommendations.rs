//! Recommendation API — `/v1/recommendations`.
//!
//! Serves cached model recommendations per capability and supports
//! pin overrides via PUT/DELETE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::app_state::AppState;

/// `GET /v1/recommendations` — all recommendations, or filtered by `?capability=`.
///
/// Without query param: returns full capability→model map.
/// With `?capability=chat`: returns single capability recommendation.
pub async fn get_recommendation(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let recommended = state.recommended_models.read().await;
    if let Some(cap) = params.get("capability") {
        let model = recommended.get(cap).cloned();
        Json(serde_json::json!({
            "capability": cap,
            "selected": model,
        }))
    } else {
        Json(serde_json::json!({ "recommendations": *recommended }))
    }
}

/// `PUT /v1/recommendations/{capability}/pin` — pin a model for a capability.
pub async fn pin_recommendation(
    State(state): State<AppState>,
    Path(capability): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> StatusCode {
    let model = match body.get("model").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return StatusCode::BAD_REQUEST,
    };

    {
        let mut config = state.config.write().await;
        config.features.pins.insert(capability.clone(), model.clone());
    }

    // Update the cached recommendations.
    {
        let mut recommended = state.recommended_models.write().await;
        recommended.insert(capability, model);
    }

    state.emit_event("recommendations.updated", "{}").await;
    StatusCode::OK
}

/// `DELETE /v1/recommendations/{capability}/pin` — remove pin.
pub async fn unpin_recommendation(
    State(state): State<AppState>,
    Path(capability): Path<String>,
) -> StatusCode {
    {
        let mut config = state.config.write().await;
        config.features.pins.remove(&capability);
    }

    // Remove from cache — the next refresh will compute the natural recommendation.
    {
        let mut recommended = state.recommended_models.write().await;
        recommended.remove(&capability);
    }

    state.emit_event("recommendations.updated", "{}").await;
    StatusCode::OK
}
