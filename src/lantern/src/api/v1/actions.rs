//! Action proxy — forwards commands to Moss instances
//!
//! Every action handler: resolve stone endpoint → build Moss URL → proxy request → return response.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use garden_common::api_utils::ApiErrorResponse;
use serde_json::Value;

use crate::api::responses::error_response;
use crate::AppState;

/// Resolve a stone's Moss endpoint from the topology cache.
/// Returns the base URL (e.g. "http://192.168.1.10:7185").
async fn resolve_stone_endpoint(
    state: &AppState,
    stone_id: &str,
) -> Result<String, (StatusCode, Json<ApiErrorResponse>)> {
    let topology = state.topology.read().await;
    let found = topology.stones.iter().find(|(key, entry)| {
        key.as_str() == stone_id || entry.stone_id == stone_id || entry.stone_name == stone_id
    });

    match found {
        Some((_, entry)) => Ok(entry.address.http_base()),
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "STONE_NOT_FOUND",
            format!("No stone found with id or name '{}'", stone_id),
        )),
    }
}

/// POST /api/v1/garden/stones/:stone_id/services/:svc/rest
pub async fn post_service_rest(
    State(state): State<AppState>,
    Path((stone_id, svc)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = resolve_stone_endpoint(&state, &stone_id).await?;
    let url = format!("{}/api/v1/stone/services/{}/rest", endpoint, svc);

    let (status, body) = state
        .http_client
        .proxy_post(&url, Value::Object(Default::default()))
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "PROXY_ERROR",
                format!("Failed to proxy to Moss: {}", e),
            )
        })?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        Json(body),
    ))
}

/// POST /api/v1/garden/stones/:stone_id/services/:svc/wake
pub async fn post_service_wake(
    State(state): State<AppState>,
    Path((stone_id, svc)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = resolve_stone_endpoint(&state, &stone_id).await?;
    let url = format!("{}/api/v1/stone/services/{}/wake", endpoint, svc);

    let (status, body) = state
        .http_client
        .proxy_post(&url, Value::Object(Default::default()))
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "PROXY_ERROR",
                format!("Failed to proxy to Moss: {}", e),
            )
        })?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        Json(body),
    ))
}

/// POST /api/v1/garden/stones/:stone_id/offerings — deploy an offering
pub async fn post_deploy_offering(
    State(state): State<AppState>,
    Path(stone_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = resolve_stone_endpoint(&state, &stone_id).await?;
    let url = format!("{}/api/v1/stone/offerings", endpoint);

    let (status, resp) = state
        .http_client
        .proxy_post(&url, body)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "PROXY_ERROR",
                format!("Failed to proxy to Moss: {}", e),
            )
        })?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        Json(resp),
    ))
}

/// DELETE /api/v1/garden/stones/:stone_id/offerings/:name — remove an offering
pub async fn delete_offering(
    State(state): State<AppState>,
    Path((stone_id, name)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = resolve_stone_endpoint(&state, &stone_id).await?;
    let url = format!("{}/api/v1/stone/offerings/{}", endpoint, name);

    let (status, resp) = state.http_client.proxy_delete(&url).await.map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "PROXY_ERROR",
            format!("Failed to proxy to Moss: {}", e),
        )
    })?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        Json(resp),
    ))
}

/// POST /api/v1/garden/stones/:stone_id/companions/:cid/command
pub async fn post_companion_command(
    State(state): State<AppState>,
    Path((stone_id, companion_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = resolve_stone_endpoint(&state, &stone_id).await?;
    let url = format!(
        "{}/api/v1/stone/companions/{}/command",
        endpoint, companion_id
    );

    let (status, resp) = state
        .http_client
        .proxy_post(&url, body)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "PROXY_ERROR",
                format!("Failed to proxy to Moss: {}", e),
            )
        })?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        Json(resp),
    ))
}
