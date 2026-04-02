//! Secrets API — manage API keys for external services.
//!
//! GET  /v1/secrets       → list all keys (masked values)
//! POST /v1/secrets/{key} → set a key value
//! DELETE /v1/secrets/{key} → delete a key

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;

/// List all secret keys with masked values.
pub async fn list_secrets(State(state): State<AppState>) -> Response {
    let secrets = state.secrets.list_masked().await;
    (StatusCode::OK, Json(secrets)).into_response()
}

#[derive(serde::Deserialize)]
pub struct SetSecretBody {
    pub value: String,
}

/// Set a secret key value.
pub async fn set_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SetSecretBody>,
) -> Response {
    match state.secrets.set(&key, &body.value).await {
        Ok(()) => {
            tracing::info!(key = %key, "secret updated");
            (StatusCode::OK, Json(serde_json::json!({ "status": "saved", "key": key }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": { "code": "save_failed", "message": e.to_string() }
            }))).into_response()
        }
    }
}

/// Delete a secret key.
pub async fn delete_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    match state.secrets.delete(&key).await {
        Ok(()) => {
            tracing::info!(key = %key, "secret deleted");
            (StatusCode::OK, Json(serde_json::json!({ "status": "deleted", "key": key }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": { "code": "delete_failed", "message": e.to_string() }
            }))).into_response()
        }
    }
}
