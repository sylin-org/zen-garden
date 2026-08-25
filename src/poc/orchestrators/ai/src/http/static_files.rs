//! Serves the embedded dashboard SPA via `rust-embed`.
//!
//! The Vite build outputs to `dashboard/dist/`. At compile time,
//! `rust-embed` bakes every file in that directory into the binary.
//! At runtime, this handler serves them with correct MIME types and
//! cache headers:
//!
//! - Hashed assets (`/assets/*.js`, `/assets/*.css`): immutable,
//!   `max-age=31536000` (1 year).
//! - `index.html`: `no-cache` so the browser always fetches the
//!   latest entry point after a deploy.
//! - SPA fallback: any path that doesn't match a static file and
//!   isn't an API route returns `index.html`.
//!
//! This module is wired into the router as a fallback handler so
//! API routes take priority.

use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "dashboard/dist/"]
struct DashboardAssets;

/// Serve an embedded file or fall back to `index.html` for SPA routing.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first.
    if !path.is_empty() {
        if let Some(file) = DashboardAssets::get(path) {
            return file_response(path, &file);
        }
    }

    // SPA fallback: serve index.html for any unknown path.
    match DashboardAssets::get("index.html") {
        Some(file) => {
            let body = file.data.to_vec();
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                body,
            )
                .into_response()
        }
        None => {
            // Dashboard not built — return a helpful message.
            (
                StatusCode::NOT_FOUND,
                Html(
                    "<h1>Dashboard not built</h1>\
                     <p>Run <code>npm run build</code> in <code>dashboard/</code> \
                     and rebuild the Rust binary.</p>",
                ),
            )
                .into_response()
        }
    }
}

fn file_response(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = mime_from_path(path);
    let body = file.data.to_vec();

    let cache = if path.starts_with("assets/") {
        // Vite hashes asset filenames — safe to cache forever.
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(cache) {
        headers.insert(header::CACHE_CONTROL, v);
    }
    response
}

fn mime_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
