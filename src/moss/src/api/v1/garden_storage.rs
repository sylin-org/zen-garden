//! Garden-scoped storage API (STORAGE-0008)
//!
//! Name-based file operations that always route to the Primary replica.
//! Any Moss can be the entry point — if the local bank is Primary, execute
//! locally; otherwise proxy the request to the stone hosting the Primary.
//!
//! ## Routes
//!
//! ```text
//! GET    /api/v1/garden/storage/{name}/{*path}   → file read (Primary-or-proxy)
//! PUT    /api/v1/garden/storage/{name}/{*path}   → file write (Primary-or-proxy)
//! DELETE /api/v1/garden/storage/{name}/{*path}   → file delete (Primary-or-proxy)
//! HEAD   /api/v1/garden/storage/{name}/{*path}   → file metadata (Primary-or-proxy)
//! GET    /api/v1/garden/storage/{name}           → discovery (all replicas)
//! ```
//!
//! See docs/decisions/STORAGE-0008-garden-stone-api-split.md

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::constants::paths;
use garden_common::storage::SeedBankRole;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::infra::storage::{ObjectStore, SeedBankRegistry, SeedBankStore};
use crate::{error_response, AppState};

// ============================================================================
// Constants
// ============================================================================

/// Header set on proxied requests to break loops.
const HEADER_ZEN_PROXIED: &str = "X-Zen-Proxied";

// ============================================================================
// Types
// ============================================================================

/// Route decision for a seed bank operation.
enum SeedBankRoute {
    /// Execute locally — this stone hosts the Primary.
    Local { mount_path: String, name: String },
    /// Proxy to the remote stone hosting the Primary.
    Remote { endpoint: String },
}

/// Query parameters for directory / object listing (mirrors storage.rs).
#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    #[serde(default)]
    pub depth: Option<String>,
}

impl ListQueryParams {
    pub fn parse_depth(&self) -> Option<usize> {
        match self.depth.as_deref() {
            None | Some("1") => Some(1),
            Some("all") | Some("-1") => None,
            Some(s) => s.parse().ok(),
        }
    }
}

/// A single replica instance for the discovery endpoint.
#[derive(Debug, Serialize)]
pub struct SeedBankInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub bank_id: String,
    pub role: SeedBankRole,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_id: Option<String>,
    pub endpoint: String,
    pub visibility: String,
    pub health: String,
}

/// Response for the discovery endpoint.
#[derive(Debug, Serialize)]
pub struct SeedBankDiscovery {
    pub name: String,
    pub instances: Vec<SeedBankInstance>,
}

/// Object metadata (re-exported shape from storage.rs).
#[derive(Debug, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
}

/// Directory listing response.
#[derive(Debug, Serialize)]
pub struct DirectoryListResponse {
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
    pub truncated: bool,
}

/// Single entry in a directory listing.
#[derive(Debug, Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
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

fn validate_seed_bank_layout(mount_path: &str) -> Result<(), String> {
    let memories = std::path::Path::new(mount_path).join(paths::SEED_BANK_MEMORIES_DIR);
    let storage = std::path::Path::new(mount_path).join(paths::SEED_BANK_STORAGE_DIR);

    if !memories.is_dir() || !storage.is_dir() {
        return Err(
            "Seed bank is non-canonical; missing garden/memories and/or garden/storage. \
             Re-prepare the seed bank."
                .to_string(),
        );
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

/// Check if the incoming request was already proxied (loop guard).
fn is_proxied(headers: &HeaderMap) -> bool {
    headers
        .get(HEADER_ZEN_PROXIED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "true")
        .unwrap_or(false)
}

// ============================================================================
// Route resolution
// ============================================================================

/// Resolve how to reach the Primary replica for a given seed bank name.
///
/// 1. Check local registry — if we have it and it's Primary, return Local.
/// 2. If local is Dormant, or we don't have it, search beacons for the Primary.
/// 3. Fall back to any remote beacon with the name.
async fn resolve_primary_route(
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

        // Check if local bank is Primary
        let roles = state.seed_bank_roles_snapshot().await;
        let role = roles.get(name).copied().unwrap_or(SeedBankRole::Primary);

        if role == SeedBankRole::Primary {
            return Ok(SeedBankRoute::Local {
                mount_path: bank.mount_path.clone(),
                name: bank.name.clone(),
            });
        }

        // Local is Dormant — find remote Primary
        debug!(
            seed_bank = %name,
            "Local seed bank is dormant, routing to remote primary"
        );
    }

    // Search beacons for Primary
    let cache = state.storage_cache.read().await;
    if let Some((stone_id, _sb)) = cache.find_primary_by_name(name) {
        if let Some(endpoint) = cache.get_endpoint(stone_id) {
            return Ok(SeedBankRoute::Remote {
                endpoint: endpoint.to_string(),
            });
        }
    }

    // Fall back to any beacon with the name
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

/// Proxy a request to a remote stone.
async fn proxy_request(
    method: reqwest::Method,
    endpoint: &str,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default();

    let url = format!(
        "{}/{}{}",
        endpoint.trim_end_matches('/'),
        path.trim_start_matches('/'),
        if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query)
        }
    );

    let mut request = client.request(method, &url);

    // Mark as proxied to prevent loops
    request = request.header(HEADER_ZEN_PROXIED, "true");

    // Forward content-type
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

    // Forward relevant headers
    for header_name in &[
        reqwest::header::CONTENT_TYPE,
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::ETAG,
        reqwest::header::LAST_MODIFIED,
    ] {
        if let Some(value) = resp_headers.get(header_name).and_then(|v| v.to_str().ok()) {
            builder = builder.header(header_name.as_str(), value);
        }
    }

    builder.body(body.into()).unwrap()
}

// ============================================================================
// GET /api/v1/garden/storage/{name} — Discovery
// ============================================================================

/// Returns all known replicas for a seed bank name.
///
/// Combines local registry with storage cache beacons.
pub async fn discover_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<SeedBankDiscovery>>, (StatusCode, Json<ApiErrorResponse>)> {
    let mut instances = Vec::new();

    // Check local registry first
    let registry = SeedBankRegistry::scan().await.map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SCAN_FAILED",
            &e.to_string(),
        )
    })?;

    if let Some(bank) = registry.get(&name) {
        let roles = state.seed_bank_roles_snapshot().await;
        let pins = state.seed_bank_pins_snapshot().await;
        let role = roles.get(&name).copied().unwrap_or(SeedBankRole::Primary);
        let pin_id = pins.get(&name).cloned();
        let local_endpoint = state.self_entry.read().await.address.http_base();

        instances.push(SeedBankInstance {
            stone_id: state.stone_id.clone(),
            stone_name: state.stone_name.clone(),
            bank_id: bank.id.clone(),
            role,
            pinned: pin_id.is_some(),
            pin_id,
            endpoint: local_endpoint,
            visibility: bank.visibility.to_string(),
            health: "healthy".to_string(),
        });
    }

    // Add remote instances from storage cache beacons
    let cache = state.storage_cache.read().await;
    for beacon in cache.all_beacons() {
        if beacon.stone_id == state.stone_id {
            continue; // Already handled local above
        }
        for sb in &beacon.seed_banks {
            if sb.name == name {
                instances.push(SeedBankInstance {
                    stone_id: beacon.stone_id.clone(),
                    stone_name: beacon.stone_name.clone(),
                    bank_id: sb.id.clone(),
                    role: sb.role,
                    pinned: sb.pin_id.is_some(),
                    pin_id: sb.pin_id.clone(),
                    endpoint: beacon.endpoint.clone(),
                    visibility: sb.visibility.clone(),
                    health: sb.health.clone(),
                });
            }
        }
    }

    if instances.is_empty() {
        return Err(err(
            StatusCode::NOT_FOUND,
            "BANK_NOT_FOUND",
            &format!("No seed bank named '{}' found in the garden", name),
        ));
    }

    Ok(Json(ApiResponse::new(SeedBankDiscovery {
        name,
        instances,
    })))
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/{*path} — Get Object
// ============================================================================

/// Get an object or directory listing from the Primary replica.
///
/// If the path ends with `/`, or the key is empty, returns a directory listing.
/// Otherwise, returns the raw object bytes.
pub async fn get_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    Query(params): Query<ListQueryParams>,
    headers: HeaderMap,
) -> Response {
    // Loop guard
    if is_proxied(&headers) {
        let roles = state.seed_bank_roles_snapshot().await;
        let role = roles.get(&name).copied().unwrap_or(SeedBankRole::Primary);
        if role != SeedBankRole::Primary {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "PROXY_LOOP",
                "Proxied request reached a non-primary stone",
            );
        }
    }

    let route = match resolve_primary_route(&state, &name).await {
        Ok(r) => r,
        Err((status, msg)) => return error_response_raw(status, "NO_SEED_BANK", &msg),
    };

    match route {
        SeedBankRoute::Local {
            mount_path, name, ..
        } => {
            if let Err(msg) = validate_seed_bank_layout(&mount_path) {
                return error_response_raw(StatusCode::CONFLICT, "BANK_NONCANONICAL", &msg);
            }
            let store = ObjectStore::new(&mount_path);
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
                return handle_directory_listing(&store, &name, &bucket, &key, &params).await;
            }

            // Object retrieval
            match store.get_object(&bucket, &key).await {
                Ok(Some((data, meta))) => {
                    debug!(seed_bank = %name, key = %key, size = data.len(), "garden GET object success");
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, &meta.content_type)
                        .header(header::CONTENT_LENGTH, data.len())
                        .header(header::ETAG, &meta.etag)
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
        SeedBankRoute::Remote { endpoint } => {
            let query_str = if let Some(ref d) = params.depth {
                format!("depth={}", d)
            } else {
                String::new()
            };
            proxy_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("api/v1/garden/storage/{}/{}", name, path),
                &query_str,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// PUT /api/v1/garden/storage/{name}/{*path} — Put Object
// ============================================================================

/// Create or update an object on the Primary replica.
pub async fn put_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<ObjectMeta>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Loop guard
    if is_proxied(&headers) {
        let roles = state.seed_bank_roles_snapshot().await;
        let role = roles.get(&name).copied().unwrap_or(SeedBankRole::Primary);
        if role != SeedBankRole::Primary {
            return Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "PROXY_LOOP",
                "Proxied request reached a non-primary stone",
            ));
        }
    }

    let route = resolve_primary_route(&state, &name)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

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

    match route {
        SeedBankRoute::Local {
            mount_path, name, ..
        } => {
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream");

            // Build a notifying store so changelog ticks reach the SSE doorbell
            // and the replication task.
            let inner = SeedBankStore::new_public(&mount_path)
                .with_notifications(name.clone(), state.storage_tick_tx.clone());
            let store = ObjectStore::with_store(inner);

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

            debug!(seed_bank = %name, key = %key, size = body.len(), "garden PUT object success");

            Ok(Json(ApiResponse::new(ObjectMeta {
                key,
                size: body.len() as u64,
                content_type: content_type.to_string(),
                etag: result.etag,
                last_modified: chrono::Utc::now().to_rfc3339(),
            })))
        }
        SeedBankRoute::Remote { endpoint } => {
            let response = proxy_request(
                reqwest::Method::PUT,
                &endpoint,
                &format!("api/v1/garden/storage/{}/{}", name, path),
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
}

// ============================================================================
// DELETE /api/v1/garden/storage/{name}/{*path} — Delete Object
// ============================================================================

/// Delete an object from the Primary replica.
pub async fn delete_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    // Loop guard
    if is_proxied(&headers) {
        let roles = state.seed_bank_roles_snapshot().await;
        let role = roles.get(&name).copied().unwrap_or(SeedBankRole::Primary);
        if role != SeedBankRole::Primary {
            return Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "PROXY_LOOP",
                "Proxied request reached a non-primary stone",
            ));
        }
    }

    let route = resolve_primary_route(&state, &name)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

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

    match route {
        SeedBankRoute::Local {
            mount_path, name, ..
        } => {
            // Build a notifying store so changelog ticks reach the SSE doorbell
            // and the replication task.
            let inner = SeedBankStore::new_public(&mount_path)
                .with_notifications(name.clone(), state.storage_tick_tx.clone());
            let store = ObjectStore::with_store(inner);

            store.delete_object(&bucket, &key).await.map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DELETE_FAILED",
                    &e.to_string(),
                )
            })?;

            debug!(seed_bank = %name, key = %key, "garden DELETE object success");
            Ok(StatusCode::NO_CONTENT)
        }
        SeedBankRoute::Remote { endpoint } => {
            let response = proxy_request(
                reqwest::Method::DELETE,
                &endpoint,
                &format!("api/v1/garden/storage/{}/{}", name, path),
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
}

// ============================================================================
// HEAD /api/v1/garden/storage/{name}/{*path} — Head Object
// ============================================================================

/// Get object metadata from the Primary replica.
pub async fn head_object_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    // Loop guard
    if is_proxied(&headers) {
        let roles = state.seed_bank_roles_snapshot().await;
        let role = roles.get(&name).copied().unwrap_or(SeedBankRole::Primary);
        if role != SeedBankRole::Primary {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "PROXY_LOOP",
                "Proxied request reached a non-primary stone",
            );
        }
    }

    let route = match resolve_primary_route(&state, &name).await {
        Ok(r) => r,
        Err((status, msg)) => return error_response_raw(status, "NO_SEED_BANK", &msg),
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

    match route {
        SeedBankRoute::Local { mount_path, .. } => {
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
            proxy_request(
                reqwest::Method::HEAD,
                &endpoint,
                &format!("api/v1/garden/storage/{}/{}", name, path),
                "",
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// Directory / Bucket listing helpers
// ============================================================================

async fn handle_bucket_listing(store: &ObjectStore) -> Response {
    match store.list_buckets().await {
        Ok(buckets) => {
            let entries = buckets
                .into_iter()
                .map(|name| DirectoryEntry {
                    name,
                    entry_type: "dir".to_string(),
                    size: None,
                    modified: None,
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
    seed_bank_name: &str,
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
                seed_bank = %seed_bank_name,
                prefix = %prefix,
                depth = ?max_depth,
                count = response.entries.len(),
                "garden directory listing"
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
