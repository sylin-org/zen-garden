//! S3 object operations on managed storage
//!
//! Objects live under `.zen-garden/storage/{bucket}/{key}`.
//! Resolution via `StorageResolver`; local I/O via `ObjectStore` accessors.

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::storage::StorageRole;
use tracing::debug;

use crate::AppState;
use crate::domain::storage_service::StorageRoute;
use crate::infra::storage::handle::StorageResolver;

use super::{
    DirectoryEntry, DirectoryListResponse, ListQueryParams, ObjectMeta, err, error_response_raw,
    has_path_traversal, is_proxied, proxy_request,
};

// ============================================================================
// Helpers
// ============================================================================

fn parse_object_path(path: &str) -> (String, String) {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return (String::new(), String::new());
    }
    let mut parts = path.splitn(2, '/');
    let bucket = parts.next().unwrap_or("").to_string();
    let key = parts.next().unwrap_or("").to_string();
    (bucket, key)
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/objects/{*path}
// ============================================================================

/// Read an S3 object or list objects/buckets.
pub async fn get_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    Query(params): Query<ListQueryParams>,
    headers: HeaderMap,
) -> Response {
    if is_proxied(&headers)
        && let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await
        && local.role != StorageRole::Primary
    {
        return error_response_raw(
            StatusCode::SERVICE_UNAVAILABLE,
            "PROXY_LOOP",
            "Proxied request reached a non-primary stone",
        );
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None, // reads don't need tick
    };
    let handle = match resolver.for_read(&name).await {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_read() {
        let (bucket, key) = parse_object_path(&path);

        if bucket.is_empty() {
            return handle_bucket_listing(&store).await;
        }

        if has_path_traversal(&bucket) {
            return error_response_raw(
                StatusCode::BAD_REQUEST,
                "INVALID_PATH",
                "Bucket contains invalid path segments",
            );
        }
        if !key.is_empty() && has_path_traversal(&key) {
            return error_response_raw(
                StatusCode::BAD_REQUEST,
                "INVALID_PATH",
                "Object key contains invalid path segments",
            );
        }

        // Directory listing
        if path.ends_with('/') || key.is_empty() {
            return handle_directory_listing(&store, handle.storage_name(), &bucket, &key, &params)
                .await;
        }

        // Object retrieval
        match store.get_object(&bucket, &key).await {
            Ok(Some((data, meta))) => {
                debug!(storage = %handle.storage_name(), key = %key, size = data.len(), "garden GET object");
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, &meta.content_type)
                    .header(header::CONTENT_LENGTH, data.len())
                    .header(header::ETAG, &meta.etag)
                    .body(data.into())
                    .unwrap()
            }
            Ok(None) => error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "Object not found"),
            Err(e) => error_response_raw(
                StatusCode::INTERNAL_SERVER_ERROR,
                "GET_FAILED",
                &e.to_string(),
            ),
        }
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let query_str = if let Some(ref d) = params.depth {
            format!("depth={}", d)
        } else {
            String::new()
        };
        proxy_request(
            reqwest::Method::GET,
            target,
            &format!("api/v1/garden/storage/{}/objects/{}", name, path),
            &query_str,
            &headers,
            None,
        )
        .await
    }
}

// ============================================================================
// PUT /api/v1/garden/storage/{name}/objects/{*path}
// ============================================================================

/// Create or update an S3 object on the Primary replica.
pub async fn put_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> crate::api::ApiResult<ObjectMeta> {
    if is_proxied(&headers)
        && let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await
        && local.role != StorageRole::Primary
    {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "PROXY_LOOP",
            "Proxied request reached a non-primary stone",
        ));
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: Some(state.current.storage.coordination.tick.raw.clone()),
    };
    let handle = resolver.for_write(&name).await.map_err(|e| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_STORAGE",
            &e.to_string(),
        )
    })?;

    let (bucket, key) = parse_object_path(&path);
    if bucket.is_empty() || key.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket and key are required",
        ));
    }
    if has_path_traversal(&bucket) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket contains invalid path segments",
        ));
    }
    if has_path_traversal(&key) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Object key contains invalid path segments",
        ));
    }

    if let Some(store) = handle.object_store_for_write() {
        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream");

        let result = store
            .put_object(&bucket, &key, content_type, &body)
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "PUT_FAILED",
                    &e.to_string(),
                )
            })?;

        debug!(storage = %handle.storage_name(), key = %key, size = body.len(), "garden PUT object");

        crate::api::ok(ObjectMeta {
            key,
            size: body.len() as u64,
            content_type: content_type.to_string(),
            etag: result.etag,
            last_modified: chrono::Utc::now().to_rfc3339(),
        })
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let response = proxy_request(
            reqwest::Method::PUT,
            target,
            &format!("api/v1/garden/storage/{}/objects/{}", name, path),
            "",
            &headers,
            Some(body),
        )
        .await;

        if response.status() != StatusCode::OK && response.status() != StatusCode::CREATED {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Failed to store object on primary",
            ));
        }
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        let data: ApiResponse<ObjectMeta> = serde_json::from_slice(&bytes)
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        Ok(Json(data))
    }
}

// ============================================================================
// DELETE /api/v1/garden/storage/{name}/objects/{*path}
// ============================================================================

/// Delete an S3 object from the Primary replica.
pub async fn delete_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    if is_proxied(&headers)
        && let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await
        && local.role != StorageRole::Primary
    {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "PROXY_LOOP",
            "Proxied request reached a non-primary stone",
        ));
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: Some(state.current.storage.coordination.tick.raw.clone()),
    };
    let handle = resolver.for_write(&name).await.map_err(|e| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_STORAGE",
            &e.to_string(),
        )
    })?;

    let (bucket, key) = parse_object_path(&path);
    if bucket.is_empty() || key.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket and key are required",
        ));
    }
    if has_path_traversal(&bucket) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket contains invalid path segments",
        ));
    }
    if has_path_traversal(&key) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Object key contains invalid path segments",
        ));
    }

    if let Some(store) = handle.object_store_for_write() {
        store.delete_object(&bucket, &key).await.map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DELETE_FAILED",
                &e.to_string(),
            )
        })?;
        debug!(storage = %handle.storage_name(), key = %key, "garden DELETE object");
        Ok(StatusCode::NO_CONTENT)
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let response = proxy_request(
            reqwest::Method::DELETE,
            target,
            &format!("api/v1/garden/storage/{}/objects/{}", name, path),
            "",
            &headers,
            None,
        )
        .await;

        if response.status() != StatusCode::NO_CONTENT && response.status() != StatusCode::OK {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Failed to delete object on primary",
            ));
        }
        Ok(StatusCode::NO_CONTENT)
    }
}

// ============================================================================
// HEAD /api/v1/garden/storage/{name}/objects/{*path}
// ============================================================================

/// Get S3 object metadata from the Primary replica.
pub async fn head_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if is_proxied(&headers)
        && let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await
        && local.role != StorageRole::Primary
    {
        return error_response_raw(
            StatusCode::SERVICE_UNAVAILABLE,
            "PROXY_LOOP",
            "Proxied request reached a non-primary stone",
        );
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };
    let handle = match resolver.for_read(&name).await {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            );
        }
    };

    let (bucket, key) = parse_object_path(&path);
    if bucket.is_empty() || key.is_empty() {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket and key are required",
        );
    }
    if has_path_traversal(&bucket) {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket contains invalid path segments",
        );
    }
    if has_path_traversal(&key) {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Object key contains invalid path segments",
        );
    }

    if let Some(store) = handle.object_store_for_read() {
        match store.head_object(&bucket, &key).await {
            Ok(Some(meta)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, &meta.content_type)
                .header(header::CONTENT_LENGTH, meta.size)
                .header(header::ETAG, &meta.etag)
                .header(header::LAST_MODIFIED, &meta.last_modified)
                .body("".into())
                .unwrap(),
            Ok(None) => error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "Object not found"),
            Err(e) => error_response_raw(
                StatusCode::INTERNAL_SERVER_ERROR,
                "HEAD_FAILED",
                &e.to_string(),
            ),
        }
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        proxy_request(
            reqwest::Method::HEAD,
            target,
            &format!("api/v1/garden/storage/{}/objects/{}", name, path),
            "",
            &headers,
            None,
        )
        .await
    }
}

// ============================================================================
// Listing helpers
// ============================================================================

use crate::infra::storage::ObjectStore;

async fn handle_bucket_listing(store: &ObjectStore) -> Response {
    match store.list_buckets().await {
        Ok(buckets) => {
            let entries = buckets
                .into_iter()
                .map(|(name, created)| DirectoryEntry {
                    name,
                    entry_type: "dir".to_string(),
                    size: None,
                    modified: Some(created.to_rfc3339()),
                })
                .collect::<Vec<_>>();
            let response = DirectoryListResponse {
                path: "/".to_string(),
                entries,
                truncated: false,
            };
            match serde_json::to_string(&ApiResponse::new(response)) {
                Ok(body) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body.into())
                    .unwrap(),
                Err(e) => error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SERIALIZE_FAILED",
                    &e.to_string(),
                ),
            }
        }
        Err(e) => error_response_raw(
            StatusCode::INTERNAL_SERVER_ERROR,
            "LIST_FAILED",
            &e.to_string(),
        ),
    }
}

async fn handle_directory_listing(
    store: &ObjectStore,
    storage_name: &str,
    bucket: &str,
    prefix: &str,
    params: &ListQueryParams,
) -> Response {
    let max_depth = params.parse_depth();
    let delimiter = if max_depth == Some(1) {
        Some("/")
    } else {
        None
    };

    match store
        .list_objects(bucket, Some(prefix), delimiter, None, 1000)
        .await
    {
        Ok(result) => {
            let mut entries: Vec<DirectoryEntry> = Vec::new();

            for obj in &result.contents {
                let name = obj.key.strip_prefix(prefix).unwrap_or(&obj.key);
                if let Some(depth) = max_depth {
                    let path_depth = name.matches('/').count() + 1;
                    if path_depth > depth {
                        continue;
                    }
                }
                entries.push(DirectoryEntry {
                    name: name.to_string(),
                    entry_type: "file".to_string(),
                    size: Some(obj.size),
                    modified: Some(obj.last_modified.clone()),
                });
            }

            for prefix_path in &result.common_prefixes {
                let name = prefix_path.strip_prefix(prefix).unwrap_or(prefix_path);
                entries.push(DirectoryEntry {
                    name: name.to_string(),
                    entry_type: "dir".to_string(),
                    size: None,
                    modified: None,
                });
            }

            let response = DirectoryListResponse {
                path: format!("{}/{}", bucket, prefix),
                entries,
                truncated: result.is_truncated,
            };

            debug!(
                storage = %storage_name,
                prefix = %prefix,
                depth = ?max_depth,
                count = response.entries.len(),
                "garden object listing"
            );

            match serde_json::to_string(&ApiResponse::new(response)) {
                Ok(json) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json.into())
                    .unwrap(),
                Err(e) => error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SERIALIZE_FAILED",
                    &e.to_string(),
                ),
            }
        }
        Err(e) => error_response_raw(
            StatusCode::INTERNAL_SERVER_ERROR,
            "LIST_FAILED",
            &e.to_string(),
        ),
    }
}
