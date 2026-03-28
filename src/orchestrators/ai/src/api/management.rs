//! Ollama model management endpoints — show, pull, delete.
//!
//! These use a shared OllamaClient to forward management commands to
//! the correct Ollama instance. The client is the same one used by the
//! OllamaOffering adapter — constructed once and shared.

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;
use crate::domain::types::OfferingKind;
use crate::offerings::ollama::client::OllamaClient;

/// Shared Ollama client for management operations.
/// Constructed lazily on first use.
static OLLAMA_CLIENT: std::sync::LazyLock<OllamaClient> =
    std::sync::LazyLock::new(OllamaClient::new);

/// `POST /api/show` — forward to an Ollama instance.
pub async fn ollama_show(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let model = body
        .get("name")
        .or_else(|| body.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if model.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing model name").into_response();
    }

    let endpoint = match find_ollama_with_model(&state, model).await {
        Some(ep) => ep,
        None => return (StatusCode::NOT_FOUND, "model not found").into_response(),
    };

    match OLLAMA_CLIENT.show_model(&endpoint, model).await {
        Ok(show) => Json(serde_json::json!(show)).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

/// `POST /api/pull` — pull a model on a healthy Ollama instance.
pub async fn ollama_pull(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let model = match body
        .get("name")
        .or_else(|| body.get("model"))
        .and_then(|v| v.as_str())
    {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return (StatusCode::BAD_REQUEST, "missing model name").into_response(),
    };

    let endpoint = match find_any_healthy_ollama(&state).await {
        Some(ep) => ep,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "no healthy Ollama instance")
                .into_response()
        }
    };

    // Consume the pull stream and collect into bytes.
    // The Ollama pull stream is NDJSON progress events — typically small.
    // For a production pull proxy, this should stream through; for now
    // we collect to avoid lifetime issues with the stream capturing &str.
    use futures_util::StreamExt;
    match OLLAMA_CLIENT.pull_model(&endpoint, &model).await {
        Ok(mut stream) => {
            let mut chunks = Vec::new();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => chunks.push(bytes),
                    Err(e) => {
                        return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
                    }
                }
            }
            let body_bytes: Vec<u8> = chunks.into_iter().flat_map(|b| b.to_vec()).collect();
            Response::builder()
                .header("content-type", "application/x-ndjson")
                .body(Body::from(body_bytes))
                .unwrap_or_default()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

/// `DELETE /api/delete` — delete a model from an Ollama instance.
pub async fn ollama_delete(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let model = body
        .get("name")
        .or_else(|| body.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if model.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing model name").into_response();
    }

    let endpoint = match find_ollama_with_model(&state, model).await {
        Some(ep) => ep,
        None => return (StatusCode::NOT_FOUND, "model not found").into_response(),
    };

    match OLLAMA_CLIENT.delete_model(&endpoint, model).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

/// Find a healthy Ollama instance that has a specific model.
async fn find_ollama_with_model(state: &AppState, model: &str) -> Option<String> {
    let instances = state.instances.read().await;
    instances
        .values()
        .find(|i| {
            i.kind == OfferingKind::Ollama
                && i.health.is_healthy()
                && i.models_available.iter().any(|m| m == model)
        })
        .map(|i| i.endpoint.clone())
}

/// Find any healthy Ollama instance (for pull operations).
async fn find_any_healthy_ollama(state: &AppState) -> Option<String> {
    let instances = state.instances.read().await;
    instances
        .values()
        .find(|i| i.kind == OfferingKind::Ollama && i.health.is_healthy())
        .map(|i| i.endpoint.clone())
}
