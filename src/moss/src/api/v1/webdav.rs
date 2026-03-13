//! WebDAV file access (STORAGE-0009 Phase 3)
//!
//! Serves managed storage content via WebDAV (RFC 4918).
//! Each storage is accessible at `/dav/{name}/`.
//!
//! ## Architecture
//!
//! Thin handler — resolves storage name via `StorageService`, then:
//! - **Local**: delegates to `dav-server` with `LocalFs` backend
//! - **Remote**: proxies the HTTP request to the hosting stone
//!
//! `LocalFs` is configured with `public=false`, which hides dotfiles
//! (`.zen-garden/`) from directory listings and blocks direct access.
//! The `Zen Garden` symlink (not a dotfile) remains visible — intentional
//! transparency per ADR STORAGE-0009.
//!
//! ## Changelog
//!
//! After a successful mutation (PUT, DELETE, MKCOL, MOVE, COPY),
//! the handler records a changelog entry via `ContentStore` so
//! replication picks up the change.
//!
//! ## Routes
//!
//! ```text
//! {any method} /dav/{name}/{*path}    → WebDAV operations
//! ```

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
};
use dav_server::{fakels::FakeLs, localfs::LocalFs, DavHandler};
use futures_util::StreamExt;
use garden_common::storage::ChangelogEntry;
use tracing::{debug, warn};

use crate::infra::storage::handle::StorageResolver;
use crate::AppState;

use super::garden_storage::HEADER_ZEN_PROXIED;

// ============================================================================
// Handler
// ============================================================================

/// Handle all WebDAV requests for `/dav/{name}/{*path}`.
///
/// Extracts the storage name from the URI, resolves via StorageService,
/// then serves locally or proxies to the remote stone.
pub async fn handle_webdav(
    State(state): State<AppState>,
    request: Request,
) -> Response {
    let uri_path = request.uri().path().to_string();
    let method = request.method().clone();

    // Extract storage name from /dav/{name}/...
    let Some(storage_name) = extract_storage_name(&uri_path) else {
        return (
            StatusCode::BAD_REQUEST,
            "Storage name required: /dav/{name}/",
        )
            .into_response();
    };

    // Block access to restricted paths (managed metadata, OS internals)
    let rel_path = extract_rel_path(&uri_path, storage_name);
    if garden_common::constants::storage::share::is_blocked_path(&rel_path) {
        return (
            StatusCode::FORBIDDEN,
            "Access to managed storage internals is not allowed",
        )
            .into_response();
    }

    // Check proxy loop guard
    let is_proxied = request
        .headers()
        .get(HEADER_ZEN_PROXIED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "true")
        .unwrap_or(false);

    let is_mutation = is_write_method(&method);

    // Resolve storage routing
    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: if is_mutation {
            Some(state.orchestration.storage.tick.raw.clone())
        } else {
            None
        },
    };

    let handle = if is_mutation {
        resolver.for_write(storage_name).await
    } else {
        resolver.for_read(storage_name).await
    };

    let handle = match handle {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Storage '{}' not found: {}", storage_name, e),
            )
                .into_response();
        }
    };

    if handle.is_local() {
        if is_proxied && handle.mount_path().is_some() {
            // Check role via local storage lookup
            if let Some(local) = crate::domain::storage_service::StorageRoute::find_local(
                storage_name,
                &state.current.storage.volumes,
            )
            .await
            {
                if local.role != garden_common::storage::StorageRole::Primary {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Proxied request reached a non-primary stone",
                    )
                        .into_response();
                }
            }
        }

        serve_local(&handle, storage_name, &method, &rel_path, request).await
    } else {
        if is_proxied {
            return (StatusCode::BAD_GATEWAY, "Proxy loop detected").into_response();
        }
        let endpoint = handle.remote_endpoint().unwrap();
        proxy_webdav(endpoint, storage_name, request).await
    }
}

// ============================================================================
// Local serving
// ============================================================================

/// Serve a WebDAV request from local storage via `dav-server`.
async fn serve_local(
    handle: &crate::infra::storage::handle::StorageHandle,
    storage_name: &str,
    method: &Method,
    rel_path: &str,
    request: Request,
) -> Response {
    let mount_path = handle.mount_path().unwrap();
    let dav = DavHandler::builder()
        .filesystem(LocalFs::new(mount_path, false, false, false))
        .locksystem(FakeLs::new())
        .strip_prefix(format!("/dav/{}", storage_name))
        .build_handler();

    let response = dav.handle(request).await;
    let status = response.status();

    // Record changelog for successful mutations
    if is_write_method(method) && status.is_success() {
        if let Some(content_store) = handle.content_store_for_write() {
            record_changelog(&content_store, method, rel_path).await;
        }
    }

    if status.is_success() {
        debug!(
            storage = %storage_name,
            method = %method,
            path = %rel_path,
            status = %status.as_u16(),
            "WebDAV request served"
        );
    }

    response.into_response()
}

// ============================================================================
// Proxy to remote stone
// ============================================================================

/// Forward a WebDAV request to the remote stone hosting the Primary.
async fn proxy_webdav(endpoint: &str, storage_name: &str, request: Request) -> Response {
    let (parts, body) = request.into_parts();

    // Build target URL preserving the /dav/{name}/... path
    let path = parts.uri.path();
    let query = parts.uri.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let url = format!("{}{}{}", endpoint.trim_end_matches('/'), path, query);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default();

    // Map the HTTP method — reqwest doesn't know WebDAV methods natively
    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    let mut req = client.request(method, &url);
    req = req.header(HEADER_ZEN_PROXIED, "true");

    // Forward all headers except host
    for (name, value) in &parts.headers {
        if name == header::HOST {
            continue;
        }
        if let Ok(v) = value.to_str() {
            req = req.header(name.as_str(), v);
        }
    }

    // Forward body
    let body_bytes = match axum::body::to_bytes(body, 200 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "Failed to read request body for proxy");
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };
    if !body_bytes.is_empty() {
        req = req.body(body_bytes);
    }

    // Send proxied request
    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                storage = %storage_name,
                endpoint = %endpoint,
                error = %e,
                "WebDAV proxy failed"
            );
            return (StatusCode::BAD_GATEWAY, format!("Proxy error: {}", e)).into_response();
        }
    };

    // Convert reqwest response → axum response (streaming — A11j Wave 3)
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();

    let mut builder = Response::builder().status(status);

    // Forward response headers
    for (name, value) in resp_headers.iter() {
        if let Ok(v) = value.to_str() {
            builder = builder.header(name.as_str(), v);
        }
    }

    // Stream the response body instead of buffering the entire payload
    let stream = response
        .bytes_stream()
        .map(|r| r.map_err(std::io::Error::other));
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build proxy response").into_response()
        })
}

// ============================================================================
// Changelog recording
// ============================================================================

/// Record a changelog entry after a successful WebDAV mutation.
///
/// Best-effort — failures are logged, never propagated.
async fn record_changelog(
    content_store: &crate::infra::storage::ContentStore,
    method: &Method,
    rel_path: &str,
) {
    if rel_path.is_empty() {
        return;
    }

    // Skip changelog for blocked paths (shouldn't reach here, but safety)
    if garden_common::constants::storage::share::is_blocked_path(rel_path) {
        return;
    }

    let entry = match method.as_str() {
        "PUT" => {
            // We don't know the exact size; use 0 — replication fetches the file anyway
            ChangelogEntry::modified(rel_path, 0)
        }
        "MKCOL" => ChangelogEntry::created(rel_path, 0),
        "DELETE" => ChangelogEntry::deleted(rel_path),
        "MOVE" | "COPY" => {
            // MOVE/COPY affect both source and destination. Record destination as modified.
            // Source deletion (for MOVE) will be caught by the next replication scan.
            ChangelogEntry::modified(rel_path, 0)
        }
        _ => return,
    };

    content_store.record_external_change(&entry).await;
    debug!(method = %method, path = %rel_path, "WebDAV changelog recorded");
}

// ============================================================================
// Path helpers
// ============================================================================

/// Extract storage name from `/dav/{name}/...`.
fn extract_storage_name(uri_path: &str) -> Option<&str> {
    let trimmed = uri_path.strip_prefix("/dav/").unwrap_or(uri_path.strip_prefix("/dav").unwrap_or(""));
    let name = trimmed.split('/').next().unwrap_or("");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Extract the relative path after `/dav/{name}/`.
fn extract_rel_path(uri_path: &str, storage_name: &str) -> String {
    let prefix = format!("/dav/{}/", storage_name);
    uri_path
        .strip_prefix(&prefix)
        .or_else(|| uri_path.strip_prefix(&format!("/dav/{}", storage_name)))
        .unwrap_or("")
        .to_string()
}


/// Whether the HTTP method is a mutation (write) operation.
fn is_write_method(method: &Method) -> bool {
    matches!(
        method,
        &Method::PUT | &Method::DELETE | &Method::POST | &Method::PATCH
    ) || matches!(method.as_str(), "MKCOL" | "MOVE" | "COPY" | "LOCK" | "UNLOCK")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_storage_name() {
        assert_eq!(extract_storage_name("/dav/personal/"), Some("personal"));
        assert_eq!(extract_storage_name("/dav/personal"), Some("personal"));
        assert_eq!(
            extract_storage_name("/dav/zen-garden/Photos/cat.jpg"),
            Some("zen-garden")
        );
        assert_eq!(extract_storage_name("/dav/"), None);
        assert_eq!(extract_storage_name("/dav"), None);
    }

    #[test]
    fn test_extract_rel_path() {
        assert_eq!(
            extract_rel_path("/dav/personal/Photos/cat.jpg", "personal"),
            "Photos/cat.jpg"
        );
        assert_eq!(
            extract_rel_path("/dav/personal/", "personal"),
            ""
        );
        assert_eq!(
            extract_rel_path("/dav/personal", "personal"),
            ""
        );
    }

    #[test]
    fn test_is_blocked_path() {
        use garden_common::constants::storage::share::is_blocked_path;
        assert!(is_blocked_path(".zen-garden/manifest.json"));
        assert!(is_blocked_path("/.zen-garden/"));
        assert!(is_blocked_path("foo/.zen-garden/bar"));
        assert!(is_blocked_path("$RECYCLE.BIN/file.txt"));
        assert!(is_blocked_path("/$RECYCLE.BIN"));
        assert!(is_blocked_path("System Volume Information/WPSettings.dat"));
        assert!(is_blocked_path("Zen Garden/manifest.json"));
        assert!(!is_blocked_path("Photos/vacation.jpg"));
        assert!(!is_blocked_path(""));
    }

    #[test]
    fn test_is_write_method() {
        assert!(is_write_method(&Method::PUT));
        assert!(is_write_method(&Method::DELETE));
        assert!(is_write_method(&Method::from_bytes(b"MKCOL").unwrap()));
        assert!(is_write_method(&Method::from_bytes(b"MOVE").unwrap()));
        assert!(!is_write_method(&Method::GET));
        assert!(!is_write_method(&Method::HEAD));
        assert!(!is_write_method(&Method::from_bytes(b"PROPFIND").unwrap()));
    }
}
