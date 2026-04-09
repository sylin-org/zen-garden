//! Preferences HTTP endpoints (ORCH-0030 §8, commit 12).
//!
//! - `GET  /v1/preferences` — the full flat map
//! - `PUT  /v1/preferences` — merge semantics (partial update)
//! - `DELETE /v1/preferences/{key}` — remove a specific key

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::app_state::AppState;

/// `GET /v1/preferences` — return the full preferences map.
pub async fn get_preferences(State(state): State<AppState>) -> Response {
    let prefs = state.preferences.get_all().await;
    Json(json!(prefs)).into_response()
}

/// `PUT /v1/preferences` — merge new values into the preferences.
/// Body is a flat JSON object of dotted field paths to values.
pub async fn put_preferences(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Response {
    let updates: HashMap<String, Value> = match body.as_object() {
        Some(obj) => obj
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "validation_failed",
                    "message": "Body must be a JSON object of field paths to values"
                })),
            )
                .into_response();
        }
    };

    if updates.is_empty() {
        return (StatusCode::OK, Json(json!({}))).into_response();
    }

    state.preferences.merge(updates).await;
    let prefs = state.preferences.get_all().await;
    Json(json!(prefs)).into_response()
}

/// `DELETE /v1/preferences/{key}` — remove a specific key.
pub async fn delete_preference(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    // URL-decode dots: the path uses dotted keys like "image.width"
    // which are a single path segment.
    if state.preferences.remove(&key).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "message": format!("Preference key '{key}' not found")
            })),
        )
            .into_response()
    }
}
