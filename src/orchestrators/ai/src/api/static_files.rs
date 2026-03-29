//! Embedded dashboard static file server.
//!
//! Serves the built React dashboard from `rust-embed` at compile time.
//! All dashboard files (HTML, JS, CSS, assets) are baked into the binary.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "dashboard/dist/"]
struct DashboardAssets;

/// Serve the dashboard SPA index.html.
pub async fn index() -> impl IntoResponse {
    match DashboardAssets::get("index.html") {
        Some(content) => Html(content.data.to_vec()).into_response(),
        None => (StatusCode::NOT_FOUND, "dashboard not built").into_response(),
    }
}

/// Serve a static asset by path.
pub async fn asset(Path(path): Path<String>) -> impl IntoResponse {
    match DashboardAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(content.data.to_vec()))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        }
        None => {
            // SPA fallback: serve index.html for client-side routes
            match DashboardAssets::get("index.html") {
                Some(content) => Html(content.data.to_vec()).into_response(),
                None => (StatusCode::NOT_FOUND, "not found").into_response(),
            }
        }
    }
}
