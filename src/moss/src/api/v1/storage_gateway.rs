//! Garden-scoped storage gateway (non-S3)
//!
//! Exposes a simple REST surface for SDKs:
//! - PUT/GET/HEAD/DELETE /api/v1/storage/{path}
//! - GET /api/v1/storage (list buckets)
//!
//! Paths are relative to the seed bank storage root: {mount}/garden/storage/{path}
//! The first path segment is treated as the bucket.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::constants::headers::HEADER_SEED_BANK;
use garden_common::constants::paths;
use garden_common::storage::DEFAULT_SEED_BANK_NAME;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::v1::storage::ObjectMeta;
use crate::infra::storage::{ObjectStore, SeedBankRegistry};
use crate::{error_response, AppState};

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_MAX_KEYS: usize = 1000;
const MAX_MAX_KEYS: usize = 1000;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Default, Deserialize)]
pub struct StorageListQuery {
    /// Only return keys with this prefix
    pub prefix: Option<String>,
    /// Delimiter for grouping keys
    pub delimiter: Option<String>,
    /// Start listing after this key
    pub marker: Option<String>,
    /// Maximum keys to return (default: 1000)
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    /// Force list behavior
    pub list: Option<bool>,
    /// Optional seed bank selector
    #[serde(rename = "seed-bank")]
    pub seed_bank: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StorageListResponse {
    pub bucket: String,
    pub prefix: String,
    pub objects: Vec<ObjectMeta>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_marker: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BucketListResponse {
    pub buckets: Vec<String>,
}

enum SeedBankRoute {
    Local { mount_path: String },
    Remote { endpoint: String },
}

// ============================================================================
// Helpers
// ============================================================================

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

fn error_response_raw(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message
        }
    });
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string().into())
        .unwrap()
}

fn get_seed_bank_name(headers: &HeaderMap, query: &StorageListQuery) -> Option<String> {
    if let Some(name) = headers.get(HEADER_SEED_BANK).and_then(|v| v.to_str().ok()) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    query.seed_bank.clone()
}

fn validate_seed_bank_layout(mount_path: &str) -> Result<(), String> {
    let memories = std::path::Path::new(mount_path).join(paths::SEED_BANK_MEMORIES_DIR);
    let storage = std::path::Path::new(mount_path).join(paths::SEED_BANK_STORAGE_DIR);

    if !memories.is_dir() || !storage.is_dir() {
        return Err("Seed bank is non-canonical; missing garden/memories and/or garden/storage. Re-prepare the seed bank.".to_string());
    }

    Ok(())
}

fn has_path_traversal(value: &str) -> bool {
    if value.contains('\\') {
        return true;
    }
    std::path::Path::new(value).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

struct PathValidationError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

fn validate_bucket(bucket: &str) -> Result<(), PathValidationError> {
    if bucket.is_empty() {
        return Err(PathValidationError {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_BUCKET",
            message: "Bucket name cannot be empty",
        });
    }
    if has_path_traversal(bucket) {
        return Err(PathValidationError {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_PATH",
            message: "Bucket contains invalid path segments",
        });
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), PathValidationError> {
    if key.is_empty() {
        return Err(PathValidationError {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_KEY",
            message: "Object key cannot be empty",
        });
    }
    if has_path_traversal(key) {
        return Err(PathValidationError {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_PATH",
            message: "Object key contains invalid path segments",
        });
    }
    Ok(())
}

fn path_error_raw(err: PathValidationError) -> Response {
    error_response_raw(err.status, err.code, err.message)
}

fn path_error_api(validation_error: PathValidationError) -> (StatusCode, Json<ApiErrorResponse>) {
    err(
        validation_error.status,
        validation_error.code,
        validation_error.message,
    )
}

fn parse_storage_path(path: &str) -> (String, String) {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return (String::new(), String::new());
    }
    let mut parts = path.splitn(2, '/');
    let bucket = parts.next().unwrap_or("").to_string();
    let key = parts.next().unwrap_or("").to_string();
    (bucket, key)
}

fn combine_prefix(base: &str, extra: &Option<String>) -> Option<String> {
    if base.is_empty() {
        return extra.clone();
    }
    match extra {
        Some(p) if !p.is_empty() => {
            let trimmed = p.trim_start_matches('/');
            let base = if base.ends_with('/') {
                base.to_string()
            } else {
                format!("{}/", base)
            };
            Some(format!("{}{}", base, trimmed))
        }
        _ => Some(base.to_string()),
    }
}

async fn resolve_seed_bank_route(
    state: &AppState,
    name: &str,
) -> Result<SeedBankRoute, (StatusCode, String)> {
    let registry = SeedBankRegistry::scan().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to scan seed banks: {}", e),
        )
    })?;

    if let Some(bank) = registry.get(name) {
        if let Err(msg) = validate_seed_bank_layout(&bank.mount_path) {
            return Err((StatusCode::CONFLICT, msg));
        }
        return Ok(SeedBankRoute::Local {
            mount_path: bank.mount_path.clone(),
        });
    }

    let cache = state.storage_cache.read().await;
    for beacon in cache.all_beacons() {
        if beacon.stone_id == state.stone_id {
            continue;
        }
        for sb in &beacon.seed_banks {
            if sb.name == name {
                return Ok(SeedBankRoute::Remote {
                    endpoint: beacon.endpoint.clone(),
                });
            }
        }
    }

    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        format!("Seed bank '{}' not available", name),
    ))
}

async fn proxy_storage_request(
    method: reqwest::Method,
    endpoint: &str,
    path: &str,
    query: Vec<(String, String)>,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/{}",
        endpoint.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let mut request = client.request(method, url);

    if !query.is_empty() {
        request = request.query(&query);
    }

    if let Some(content_type) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(reqwest::header::CONTENT_TYPE, content_type);
    }

    if let Some(body) = body {
        request = request.body(body);
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return error_response_raw(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string());
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();

    let mut builder = Response::builder().status(status);
    if let Some(value) = resp_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    if let Some(value) = resp_headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::CONTENT_LENGTH, value);
    }
    if let Some(value) = resp_headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::ETAG, value);
    }
    if let Some(value) = resp_headers
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::LAST_MODIFIED, value);
    }

    builder.body(body.into()).unwrap()
}

// ============================================================================
// GET /api/v1/storage - List buckets
// ============================================================================

pub async fn list_buckets(
    State(state): State<AppState>,
    Query(query): Query<StorageListQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<BucketListResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let selected =
        get_seed_bank_name(&headers, &query).unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            let buckets = store.list_buckets().await.map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "LIST_FAILED",
                    &e.to_string(),
                )
            })?;
            Ok(Json(ApiResponse::new(BucketListResponse { buckets })))
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_storage_request(
                reqwest::Method::GET,
                &endpoint,
                "/api/v1/storage",
                query_params,
                &headers,
                None,
            )
            .await;

            if response.status() != StatusCode::OK {
                return Err(err(
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Failed to list buckets",
                ));
            }

            let bytes = match axum::body::to_bytes(response.into_body(), usize::MAX).await {
                Ok(b) => b,
                Err(e) => {
                    return Err(err(
                        StatusCode::BAD_GATEWAY,
                        "UPSTREAM_ERROR",
                        &e.to_string(),
                    ))
                }
            };
            let data: ApiResponse<BucketListResponse> = serde_json::from_slice(&bytes)
                .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
            Ok(Json(data))
        }
    }
}

// ============================================================================
// GET /api/v1/storage/*path - Get object or list
// ============================================================================

pub async fn get_object(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<StorageListQuery>,
    headers: HeaderMap,
) -> Response {
    let selected =
        get_seed_bank_name(&headers, &query).unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return error_response_raw(status, "NO_SEED_BANK", &msg),
    };

    let (bucket, key) = parse_storage_path(&path);
    if bucket.is_empty() {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "Bucket is required",
        );
    }
    if let Err(err) = validate_bucket(&bucket) {
        return path_error_raw(err);
    }

    let wants_list = path.ends_with('/')
        || key.is_empty()
        || query.list.unwrap_or(false)
        || query.prefix.is_some()
        || query.delimiter.is_some()
        || query.marker.is_some()
        || query.max_keys.is_some();

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);

            if wants_list {
                if has_path_traversal(&key) {
                    return error_response_raw(
                        StatusCode::BAD_REQUEST,
                        "INVALID_PATH",
                        "Prefix contains invalid path segments",
                    );
                }
                if let Some(prefix) = &query.prefix {
                    if has_path_traversal(prefix) {
                        return error_response_raw(
                            StatusCode::BAD_REQUEST,
                            "INVALID_PATH",
                            "Prefix contains invalid path segments",
                        );
                    }
                }
                if let Some(marker) = &query.marker {
                    if has_path_traversal(marker) {
                        return error_response_raw(
                            StatusCode::BAD_REQUEST,
                            "INVALID_PATH",
                            "Marker contains invalid path segments",
                        );
                    }
                }

                let max_keys = query.max_keys.unwrap_or(DEFAULT_MAX_KEYS).min(MAX_MAX_KEYS);
                let prefix = combine_prefix(&key, &query.prefix);

                match store
                    .list_objects(
                        &bucket,
                        prefix.as_deref(),
                        query.delimiter.as_deref(),
                        query.marker.as_deref(),
                        max_keys,
                    )
                    .await
                {
                    Ok(result) => {
                        let objects = result
                            .contents
                            .into_iter()
                            .map(|o| ObjectMeta {
                                key: o.key,
                                size: o.size,
                                content_type: o.content_type,
                                etag: o.etag,
                                last_modified: o.last_modified,
                            })
                            .collect();
                        let response = StorageListResponse {
                            bucket: bucket.clone(),
                            prefix: prefix.unwrap_or_default(),
                            objects,
                            common_prefixes: result.common_prefixes,
                            is_truncated: result.is_truncated,
                            next_marker: result.next_marker,
                        };
                        let body = serde_json::to_string(&ApiResponse::new(response))
                            .unwrap_or_else(|_| "{\"error\":{\"code\":\"SERIALIZE_FAILED\",\"message\":\"Failed to serialize response\"}}".to_string());
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(body.into())
                            .unwrap()
                    }
                    Err(e) => error_response_raw(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "LIST_FAILED",
                        &e.to_string(),
                    ),
                }
            } else {
                if let Err(err) = validate_key(&key) {
                    return path_error_raw(err);
                }
                match store.get_object(&bucket, &key).await {
                    Ok(Some((data, meta))) => {
                        debug!(bucket = %bucket, key = %key, size = data.len(), "GET object success");
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, &meta.content_type)
                            .header(header::CONTENT_LENGTH, data.len())
                            .header(header::ETAG, &meta.etag)
                            .header(header::LAST_MODIFIED, &meta.last_modified)
                            .body(data.into())
                            .unwrap()
                    }
                    Ok(None) => {
                        error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "Object not found")
                    }
                    Err(e) => error_response_raw(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "GET_FAILED",
                        &e.to_string(),
                    ),
                }
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if let Some(prefix) = &query.prefix {
                query_params.push(("prefix".to_string(), prefix.clone()));
            }
            if let Some(delimiter) = &query.delimiter {
                query_params.push(("delimiter".to_string(), delimiter.clone()));
            }
            if let Some(marker) = &query.marker {
                query_params.push(("marker".to_string(), marker.clone()));
            }
            if let Some(max_keys) = query.max_keys {
                query_params.push(("max-keys".to_string(), max_keys.to_string()));
            }
            if query.list.unwrap_or(false) {
                query_params.push(("list".to_string(), "true".to_string()));
            }
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            proxy_storage_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/storage/{}", path),
                query_params,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// PUT /api/v1/storage/*path - Put object
// ============================================================================

pub async fn put_object(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<StorageListQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<ObjectMeta>>, (StatusCode, Json<ApiErrorResponse>)> {
    let selected =
        get_seed_bank_name(&headers, &query).unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    let (bucket, key) = parse_storage_path(&path);
    if let Err(err) = validate_bucket(&bucket) {
        return Err(path_error_api(err));
    }
    if let Err(err) = validate_key(&key) {
        return Err(path_error_api(err));
    }

    match route {
        SeedBankRoute::Local { mount_path } => {
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream");

            let store = ObjectStore::new(&mount_path);
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

            Ok(Json(ApiResponse::new(ObjectMeta {
                key,
                size: body.len() as u64,
                content_type: content_type.to_string(),
                etag: result.etag,
                last_modified: chrono::Utc::now().to_rfc3339(),
            })))
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_storage_request(
                reqwest::Method::PUT,
                &endpoint,
                &format!("/api/v1/storage/{}", path),
                query_params,
                &headers,
                Some(body),
            )
            .await;

            if response.status() != StatusCode::OK && response.status() != StatusCode::CREATED {
                return Err(err(
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Failed to store object",
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
}

// ============================================================================
// DELETE /api/v1/storage/*path - Delete object
// ============================================================================

pub async fn delete_object(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<StorageListQuery>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    let selected =
        get_seed_bank_name(&headers, &query).unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    let (bucket, key) = parse_storage_path(&path);
    if let Err(err) = validate_bucket(&bucket) {
        return Err(path_error_api(err));
    }
    if let Err(err) = validate_key(&key) {
        return Err(path_error_api(err));
    }

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            store.delete_object(&bucket, &key).await.map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DELETE_FAILED",
                    &e.to_string(),
                )
            })?;
            Ok(StatusCode::NO_CONTENT)
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_storage_request(
                reqwest::Method::DELETE,
                &endpoint,
                &format!("/api/v1/storage/{}", path),
                query_params,
                &headers,
                None,
            )
            .await;

            if response.status() != StatusCode::NO_CONTENT && response.status() != StatusCode::OK {
                return Err(err(
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Failed to delete object",
                ));
            }
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

// ============================================================================
// HEAD /api/v1/storage/*path - Head object
// ============================================================================

pub async fn head_object(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<StorageListQuery>,
    headers: HeaderMap,
) -> Response {
    let selected =
        get_seed_bank_name(&headers, &query).unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return error_response_raw(status, "NO_SEED_BANK", &msg),
    };

    let (bucket, key) = parse_storage_path(&path);
    if let Err(err) = validate_bucket(&bucket) {
        return path_error_raw(err);
    }
    if let Err(err) = validate_key(&key) {
        return path_error_raw(err);
    }

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.head_object(&bucket, &key).await {
                Ok(Some(meta)) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, &meta.content_type)
                    .header(header::CONTENT_LENGTH, meta.size)
                    .header(header::ETAG, &meta.etag)
                    .header(header::LAST_MODIFIED, &meta.last_modified)
                    .body("".into())
                    .unwrap(),
                Ok(None) => {
                    error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "Object not found")
                }
                Err(e) => error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "HEAD_FAILED",
                    &e.to_string(),
                ),
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            proxy_storage_request(
                reqwest::Method::HEAD,
                &endpoint,
                &format!("/api/v1/storage/{}", path),
                query_params,
                &headers,
                None,
            )
            .await
        }
    }
}
