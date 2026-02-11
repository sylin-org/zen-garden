//! S3-compatible Object Storage Gateway API
//!
//! Provides S3-compatible endpoints for storing and retrieving objects from seed banks.
//! See docs/reference/s3-api-reference.md and docs/decisions/STORAGE-0002-api-structure.md.
//!
//! ## Endpoints
//!
//! ```text
//! GET  /api/v1/storage/s3              → List buckets (XML)
//! GET  /api/v1/storage/s3/:bucket      → List objects in bucket (XML)
//! PUT  /api/v1/storage/s3/:bucket/*key → Put object
//! GET  /api/v1/storage/s3/:bucket/*key → Get object (raw bytes)
//! HEAD /api/v1/storage/s3/:bucket/*key → Object metadata (headers)
//! DELETE /api/v1/storage/s3/:bucket/*key → Delete object
//! ```
//!
//! ## Headers
//!
//! - `X-Seed-Bank` - Optional. Select a specific seed bank by name.
//! - `Content-Type` - MIME type for PUT (default: application/octet-stream)
//!
//! ## Query Params
//!
//! - `seed-bank` - Optional. Select a specific seed bank by name.
//!
//! ## Response Format
//!
//! S3-compliant XML for list operations. Raw bytes for GET. Empty body with headers for HEAD.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::infra::storage::{ObjectStore, SeedBankRegistry};
use crate::AppState;
use garden_common::constants::paths;
use garden_common::constants::headers::HEADER_SEED_BANK;
use garden_common::storage::DEFAULT_SEED_BANK_NAME;

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_MAX_KEYS: usize = 1000;
const MAX_MAX_KEYS: usize = 1000;

// ============================================================================
// Helper Types
// ============================================================================

#[derive(Debug, Default, Deserialize)]
pub struct SeedBankSelector {
    #[serde(rename = "seed-bank")]
    seed_bank: Option<String>,
}

impl SeedBankSelector {
    fn name(&self) -> Option<String> {
        self.seed_bank.clone()
    }
}

enum SeedBankRoute {
    Local { mount_path: String },
    Remote { endpoint: String },
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_seed_bank_name(headers: &HeaderMap, selector: &SeedBankSelector) -> Option<String> {
    if let Some(name) = headers.get(HEADER_SEED_BANK).and_then(|v| v.to_str().ok()) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    selector.name()
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

fn validate_bucket(bucket: &str) -> Option<Response> {
    if bucket.is_empty() {
        return Some(xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket name cannot be empty"));
    }
    if has_path_traversal(bucket) {
        return Some(xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket contains invalid path segments"));
    }
    None
}

fn validate_key(key: &str) -> Option<Response> {
    if key.is_empty() {
        return Some(xml_error(StatusCode::BAD_REQUEST, "InvalidKey", "Object key cannot be empty"));
    }
    if has_path_traversal(key) {
        return Some(xml_error(StatusCode::BAD_REQUEST, "InvalidKey", "Object key contains invalid path segments"));
    }
    None
}

async fn resolve_seed_bank_route(
    state: &AppState,
    name: &str,
) -> Result<SeedBankRoute, (StatusCode, String)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to scan seed banks: {}", e)))?;

    if let Some(bank) = registry.get(name) {
        if let Err(msg) = validate_seed_bank_layout(&bank.mount_path) {
            return Err((StatusCode::CONFLICT, msg));
        }
        return Ok(SeedBankRoute::Local { mount_path: bank.mount_path.clone() });
    }

    let cache = state.storage_cache.read().await;
    for beacon in cache.all_beacons() {
        if beacon.stone_id == state.stone_id {
            continue;
        }
        for sb in &beacon.seed_banks {
            if sb.name == name {
                return Ok(SeedBankRoute::Remote { endpoint: beacon.endpoint.clone() });
            }
        }
    }

    Err((StatusCode::SERVICE_UNAVAILABLE, format!("Seed bank '{}' not available", name)))
}

/// Build XML error response
fn xml_error(status: StatusCode, code: &str, message: &str) -> Response {
    let body = format!(
        r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<Error>
    <Code>{}</Code>
    <Message>{}</Message>
</Error>"#,
        code, message
    );

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/xml")
        .body(body.into())
        .unwrap()
}

async fn proxy_s3_request(
    method: reqwest::Method,
    endpoint: &str,
    path: &str,
    query: Vec<(String, String)>,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    let client = reqwest::Client::new();
    let url = format!("{}/{}", endpoint.trim_end_matches('/'), path.trim_start_matches('/'));
    let mut request = client.request(method, url);

    if !query.is_empty() {
        request = request.query(&query);
    }

    if let Some(content_type) = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        request = request.header(reqwest::header::CONTENT_TYPE, content_type);
    }

    if let Some(body) = body {
        request = request.body(body);
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return xml_error(StatusCode::BAD_GATEWAY, "UpstreamError", &e.to_string());
        }
    };

    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();

    let mut builder = Response::builder().status(status);
    if let Some(value) = resp_headers.get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    if let Some(value) = resp_headers.get(reqwest::header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()) {
        builder = builder.header(header::CONTENT_LENGTH, value);
    }
    if let Some(value) = resp_headers.get(reqwest::header::ETAG).and_then(|v| v.to_str().ok()) {
        builder = builder.header(header::ETAG, value);
    }
    if let Some(value) = resp_headers.get(reqwest::header::LAST_MODIFIED).and_then(|v| v.to_str().ok()) {
        builder = builder.header(header::LAST_MODIFIED, value);
    }

    builder.body(body.into()).unwrap()
}

// ============================================================================
// PUT /api/v1/storage/s3/:bucket/*key - Put Object
// ============================================================================

/// Put an object to seed bank storage
pub async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Validate bucket and key
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    match route {
        SeedBankRoute::Local { mount_path } => {
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream");

            let store = ObjectStore::new(&mount_path);
            match store.put_object(&bucket, key, content_type, &body).await {
                Ok(result) => {
                    debug!(bucket = %bucket, key = %key, size = body.len(), "PUT object success");
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::ETAG, &result.etag)
                        .body("".into())
                        .unwrap()
                }
                Err(e) => {
                    warn!(error = %e, "PUT object failed");
                    xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
                }
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::PUT,
                &endpoint,
                &format!("/api/v1/storage/s3/{}/{}", bucket, key),
                query,
                &headers,
                Some(body),
            )
            .await
        }
    }
}

// ============================================================================
// GET /api/v1/storage/s3/:bucket/*key - Get Object
// ============================================================================

/// Get an object from seed bank storage
pub async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    // Validate bucket and key
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "Object key cannot be empty");
    }
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.get_object(&bucket, key).await {
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
                Ok(None) => xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &format!("Key '{}' not found", key)),
                Err(e) => {
                    warn!(error = %e, "GET object failed");
                    xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
                }
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/storage/s3/{}/{}", bucket, key),
                query,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// HEAD /api/v1/storage/s3/:bucket/*key - Head Object
// ============================================================================

/// Get object metadata without body
pub async fn head_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    // Validate bucket and key
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return Response::builder().status(StatusCode::NOT_FOUND).body("".into()).unwrap();
    }
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, _)) => return Response::builder().status(status).body("".into()).unwrap(),
    };

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.head_object(&bucket, key).await {
                Ok(Some(meta)) => {
                    debug!(bucket = %bucket, key = %key, "HEAD object success");
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, &meta.content_type)
                        .header(header::CONTENT_LENGTH, meta.size)
                        .header(header::ETAG, &meta.etag)
                        .header(header::LAST_MODIFIED, &meta.last_modified)
                        .body("".into())
                        .unwrap()
                }
                Ok(None) => Response::builder().status(StatusCode::NOT_FOUND).body("".into()).unwrap(),
                Err(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body("".into()).unwrap(),
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::HEAD,
                &endpoint,
                &format!("/api/v1/storage/s3/{}/{}", bucket, key),
                query,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// DELETE /api/v1/storage/s3/:bucket/*key - Delete Object
// ============================================================================

/// Delete an object from seed bank storage
pub async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    // Validate bucket and key
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.delete_object(&bucket, key).await {
                Ok(_) => {
                    debug!(bucket = %bucket, key = %key, "DELETE object success");
                    Response::builder().status(StatusCode::NO_CONTENT).body("".into()).unwrap()
                }
                Err(e) => {
                    warn!(error = %e, "DELETE object failed");
                    xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
                }
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::DELETE,
                &endpoint,
                &format!("/api/v1/storage/s3/{}/{}", bucket, key),
                query,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// GET /api/v1/storage/s3 - List Buckets
// ============================================================================

/// List all buckets
pub async fn list_buckets(
    State(state): State<AppState>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.list_buckets().await {
                Ok(buckets) => {
                    debug!(count = buckets.len(), "LIST buckets success");
                    let xml = build_list_all_buckets_result(&buckets);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/xml")
                        .body(xml.into())
                        .unwrap()
                }
                Err(e) => {
                    warn!(error = %e, "LIST buckets failed");
                    xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
                }
            }
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query = Vec::new();
            if selected != DEFAULT_SEED_BANK_NAME {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::GET,
                &endpoint,
                "/api/v1/storage/s3",
                query,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// GET /api/v1/storage/s3/:bucket - List Objects
// ============================================================================

/// Query parameters for list objects
#[derive(Debug, Default, Deserialize)]
pub struct ListObjectsQuery {
    /// Only return keys with this prefix
    pub prefix: Option<String>,
    /// Delimiter for grouping keys
    pub delimiter: Option<String>,
    /// Start listing after this key
    pub marker: Option<String>,
    /// Maximum keys to return (default: 1000)
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    /// Optional seed bank selector
    #[serde(rename = "seed-bank")]
    pub seed_bank: Option<String>,
}

impl ListObjectsQuery {
}

/// List objects in a bucket
pub async fn list_objects(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectsQuery>,
    headers: HeaderMap,
) -> Response {
    // Validate bucket
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }

    let selector = SeedBankSelector {
        seed_bank: query.seed_bank.clone(),
    };

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    let max_keys = query.max_keys.unwrap_or(DEFAULT_MAX_KEYS).min(MAX_MAX_KEYS);

    match route {
        SeedBankRoute::Local { mount_path } => {
            let store = ObjectStore::new(&mount_path);
            match store.list_objects(
                &bucket,
                query.prefix.as_deref(),
                query.delimiter.as_deref(),
                query.marker.as_deref(),
                max_keys,
            ).await {
                Ok(result) => {
                    debug!(bucket = %bucket, count = result.contents.len(), truncated = result.is_truncated, "LIST objects success");

                    let xml = build_list_bucket_result(
                        &bucket,
                        query.prefix.as_deref().unwrap_or(""),
                        query.marker.as_deref().unwrap_or(""),
                        max_keys,
                        query.delimiter.as_deref().unwrap_or(""),
                        &result,
                    );

                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/xml")
                        .body(xml.into())
                        .unwrap()
                }
                Err(e) => {
                    warn!(error = %e, "LIST objects failed");
                    xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
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
            query_params.push(("max-keys".to_string(), max_keys.to_string()));
            if selected != DEFAULT_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }

            proxy_s3_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/storage/s3/{}", bucket),
                query_params,
                &headers,
                None,
            )
            .await
        }
    }
}

/// Build ListBucketResult XML
fn build_list_bucket_result(
    bucket: &str,
    prefix: &str,
    marker: &str,
    max_keys: usize,
    delimiter: &str,
    result: &crate::infra::storage::ListResult,
) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"#);
    xml.push_str("\n<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");

    xml.push_str(&format!("\n  <Name>{}</Name>", escape_xml(bucket)));
    xml.push_str(&format!("\n  <Prefix>{}</Prefix>", escape_xml(prefix)));
    xml.push_str(&format!("\n  <Marker>{}</Marker>", escape_xml(marker)));
    xml.push_str(&format!("\n  <MaxKeys>{}</MaxKeys>", max_keys));

    if !delimiter.is_empty() {
        xml.push_str(&format!("\n  <Delimiter>{}</Delimiter>", escape_xml(delimiter)));
    }

    xml.push_str(&format!("\n  <IsTruncated>{}</IsTruncated>", result.is_truncated));

    for obj in &result.contents {
        xml.push_str("\n  <Contents>");
        xml.push_str(&format!("\n    <Key>{}</Key>", escape_xml(&obj.key)));
        xml.push_str(&format!("\n    <LastModified>{}</LastModified>", escape_xml(&obj.last_modified)));
        xml.push_str(&format!("\n    <ETag>{}</ETag>", escape_xml(&obj.etag)));
        xml.push_str(&format!("\n    <Size>{}</Size>", obj.size));
        xml.push_str("\n    <StorageClass>STANDARD</StorageClass>");
        xml.push_str("\n  </Contents>");
    }

    for prefix in &result.common_prefixes {
        xml.push_str("\n  <CommonPrefixes>");
        xml.push_str(&format!("\n    <Prefix>{}</Prefix>", escape_xml(prefix)));
        xml.push_str("\n  </CommonPrefixes>");
    }

    xml.push_str("\n</ListBucketResult>");
    xml
}

/// Escape special XML characters
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build ListAllMyBucketsResult XML
fn build_list_all_buckets_result(buckets: &[String]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"#);
    xml.push_str("\n<ListAllMyBucketsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");

    xml.push_str("\n  <Owner>");
    xml.push_str("\n    <ID>zen-garden</ID>");
    xml.push_str("\n    <DisplayName>zen-garden</DisplayName>");
    xml.push_str("\n  </Owner>");

    xml.push_str("\n  <Buckets>");
    for bucket in buckets {
        xml.push_str("\n    <Bucket>");
        xml.push_str(&format!("\n      <Name>{}</Name>", escape_xml(bucket)));
        xml.push_str("\n      <CreationDate>2025-01-01T00:00:00.000Z</CreationDate>");
        xml.push_str("\n    </Bucket>");
    }
    xml.push_str("\n  </Buckets>");

    xml.push_str("\n</ListAllMyBucketsResult>");
    xml
}
