//! Companion endpoints — proxy to Moss companion APIs

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use garden_common::api_utils::ApiErrorResponse;
use serde_json::Value;

use crate::api::responses::error_response;
use crate::AppState;

/// GET /api/v1/garden/stones/:stone_id/companions — list companions on a stone
pub async fn get_companions(
    State(state): State<AppState>,
    Path(stone_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let endpoint = {
        let topology = state.topology.read().await;
        let found = topology.stones.iter().find(|(key, entry)| {
            key.as_str() == stone_id || entry.stone_id == stone_id || entry.stone_name == stone_id
        });

        match found {
            Some((_, entry)) => entry.address.http_base(),
            None => {
                return Err(error_response(
                    StatusCode::NOT_FOUND,
                    "STONE_NOT_FOUND",
                    format!("No stone found with id or name '{}'", stone_id),
                ))
            }
        }
    };

    let url = format!("{}/api/v1/stone/companions", endpoint);
    let body: Value = state.http_client.get_json(&url).await.map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "PROXY_ERROR",
            format!("Failed to proxy to Moss: {}", e),
        )
    })?;

    Ok(Json(body))
}
