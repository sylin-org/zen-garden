//! `GET /v1/` — minimal sitemap.

use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

pub async fn get_sitemap() -> impl IntoResponse {
    Json(json!({
        "actions": "/v1/do",
        "catalog": "/v1/catalog",
        "media": "/v1/media",
        "jobs": "/v1/jobs",
        "recommendations": "/v1/recommendations",
        "health": "/health",
    }))
}
