//! Flush endpoints for providers and the media store.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;
use crate::domain::ids::ProviderName;
use crate::domain::media::MediaFilter;

pub async fn flush_one_provider(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let provider_name = ProviderName::new(name);
    let Some(provider) = state.directory.provider(&provider_name).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": {"code": "not_found", "message": "provider not registered"}})),
        )
            .into_response();
    };
    match provider.flush_caches().await {
        Ok(report) => Json(report).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": {"code": "upstream_error", "message": e.to_string()},
            })),
        )
            .into_response(),
    }
}

pub async fn flush_all_providers(State(state): State<AppState>) -> Response {
    let snapshot = state.directory.snapshot();
    let mut reports = serde_json::Map::new();
    for (name, _) in snapshot.providers.iter() {
        if let Some(provider) = state.directory.provider(name).await {
            match provider.flush_caches().await {
                Ok(r) => {
                    reports.insert(name.as_str().to_string(), serde_json::to_value(r).unwrap_or(json!({})));
                }
                Err(e) => {
                    reports.insert(
                        name.as_str().to_string(),
                        json!({"error": e.to_string()}),
                    );
                }
            }
        }
    }
    Json(json!({ "providers": reports })).into_response()
}

pub async fn flush_media(State(state): State<AppState>) -> Response {
    // Require an explicit `only_expired=true` filter or refuse.
    let filter = MediaFilter {
        only_expired: true,
        ..Default::default()
    };
    match state.media_store.flush(filter).await {
        Ok(report) => Json(report).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "internal_error", "message": e.to_string()}})),
        )
            .into_response(),
    }
}
