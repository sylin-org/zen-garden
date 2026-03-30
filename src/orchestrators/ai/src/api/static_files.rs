//! Embedded dashboard static file server.
//!
//! Serves the built React dashboard from `rust-embed` at compile time.
//! All dashboard files (HTML, JS, CSS, assets) are baked into the binary.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "dashboard/dist/"]
struct DashboardAssets;

/// Serve the dashboard SPA index.html.
pub async fn index() -> impl IntoResponse {
    serve_file("index.html")
}

/// Fallback handler: serves static files or falls back to index.html for SPA routes.
pub async fn fallback(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try serving the exact file first
    if !path.is_empty() && path != "index.html" {
        if let Some(resp) = try_serve_file(path) {
            return resp;
        }
    }

    // SPA fallback: serve index.html for client-side routes
    serve_file("index.html")
}

/// Try to serve a file from embedded assets. Returns None if not found.
fn try_serve_file(path: &str) -> Option<Response> {
    let content = DashboardAssets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    Some(
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
            }),
    )
}

/// Serve a known file, or 404.
fn serve_file(path: &str) -> Response {
    match DashboardAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
