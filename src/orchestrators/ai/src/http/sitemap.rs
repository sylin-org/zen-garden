//! `GET /v1/` — minimal sitemap.

use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

pub async fn get_sitemap() -> impl IntoResponse {
    Json(json!({
        "actions": "/v1/do",
        "catalog": "/v1/catalog",
        "events":  "/v1/events",
        "media":   "/v1/media",
        "jobs":    "/v1/jobs",
        "skills":  "/v1/skills",
        "resources": "/v1/resources",
        "health":  "/health",
    }))
}
