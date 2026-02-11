//! SPA static file serving via rust-embed
//!
//! Serves the React SPA from embedded assets. index.html gets no-cache headers,
//! hashed assets get immutable cache headers.

use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::infra::embedded::FrontendAssets;

/// Serve a static file from the embedded frontend assets.
///
/// Axum's `*path` wildcard captures the trailing portion after `/assets/`,
/// potentially with a leading `/`. We strip that and prepend `assets/` to
/// match the rust-embed folder layout (e.g. `assets/index-CnoF8IJU.js`).
pub async fn serve_spa(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    let clean = path.trim_start_matches('/');
    let full_path = format!("assets/{clean}");
    serve_embedded_file(&full_path)
}

/// Serve the SPA index.html (fallback for client-side routing)
pub async fn serve_index() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

fn serve_embedded_file(path: &str) -> impl IntoResponse {
    match FrontendAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();

            // index.html: always revalidate. Hashed assets: cache forever.
            let cache_control = if path == "index.html" {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (header::CACHE_CONTROL, cache_control.to_string()),
                ],
                file.data.to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}
