//! S3-compatible Object Storage Gateway API (STORAGE-0009)
//!
//! Provides S3-compatible endpoints for storing and retrieving objects.
//! Uses `StorageService` for resolution and routing.
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
//! - `X-Seed-Bank` - Optional. Select a specific storage by name.
//! - `Content-Type` - MIME type for PUT (default: application/octet-stream)
//!
//! ## Query Params
//!
//! - `seed-bank` - Optional. Select a specific storage by name.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::domain::storage_service::StorageRoute;
use crate::AppState;
use garden_common::constants::headers::HEADER_SEED_BANK;
use garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY;

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

// ============================================================================
// Helper Functions
// ============================================================================

fn get_storage_name(headers: &HeaderMap, selector: &SeedBankSelector) -> Option<String> {
    if let Some(name) = headers.get(HEADER_SEED_BANK).and_then(|v| v.to_str().ok()) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    selector.name()
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
        return Some(xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidBucket",
            "Bucket name cannot be empty",
        ));
    }
    if has_path_traversal(bucket) {
        return Some(xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidBucket",
            "Bucket contains invalid path segments",
        ));
    }
    None
}

fn validate_key(key: &str) -> Option<Response> {
    if key.is_empty() {
        return Some(xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidKey",
            "Object key cannot be empty",
        ));
    }
    if has_path_traversal(key) {
        return Some(xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidKey",
            "Object key contains invalid path segments",
        ));
    }
    None
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
            return xml_error(StatusCode::BAD_GATEWAY, "UpstreamError", &e.to_string());
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();

    let mut builder = Response::builder().status(status);
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
// PUT /api/v1/storage/s3/:bucket/*key - Put Object
// ============================================================================

/// Put an object to storage
pub async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_write(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(e) => return xml_error(StatusCode::SERVICE_UNAVAILABLE, "NoSeedBank", &e.to_string()),
    };

    match route {
        StorageRoute::Local(local) => {
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream");

            let store = local.notifying_object_store(Some(&state.orchestration.storage.tick.raw));
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
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        }
        StorageRoute::Proxy(target) => {
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::PUT,
                &target.endpoint,
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

/// Get an object from storage
pub async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return xml_error(
            StatusCode::NOT_FOUND,
            "NoSuchKey",
            "Object key cannot be empty",
        );
    }
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_read(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(e) => return xml_error(StatusCode::SERVICE_UNAVAILABLE, "NoSeedBank", &e.to_string()),
    };

    match route {
        StorageRoute::Local(local) => {
            let store = local.object_store();
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
                Ok(None) => xml_error(
                    StatusCode::NOT_FOUND,
                    "NoSuchKey",
                    &format!("Key '{}' not found", key),
                ),
                Err(e) => {
                    warn!(error = %e, "GET object failed");
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        }
        StorageRoute::Proxy(target) => {
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::GET,
                &target.endpoint,
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
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("".into())
            .unwrap();
    }
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_read(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body("".into())
                .unwrap()
        }
    };

    match route {
        StorageRoute::Local(local) => {
            let store = local.object_store();
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
                Ok(None) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body("".into())
                    .unwrap(),
                Err(_) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body("".into())
                    .unwrap(),
            }
        }
        StorageRoute::Proxy(target) => {
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::HEAD,
                &target.endpoint,
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

/// Delete an object from storage
pub async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_write(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(e) => return xml_error(StatusCode::SERVICE_UNAVAILABLE, "NoSeedBank", &e.to_string()),
    };

    match route {
        StorageRoute::Local(local) => {
            let store = local.notifying_object_store(Some(&state.orchestration.storage.tick.raw));
            match store.delete_object(&bucket, key).await {
                Ok(_) => {
                    debug!(bucket = %bucket, key = %key, "DELETE object success");
                    Response::builder()
                        .status(StatusCode::NO_CONTENT)
                        .body("".into())
                        .unwrap()
                }
                Err(e) => {
                    warn!(error = %e, "DELETE object failed");
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        }
        StorageRoute::Proxy(target) => {
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::DELETE,
                &target.endpoint,
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
    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_read(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(e) => return xml_error(StatusCode::SERVICE_UNAVAILABLE, "NoSeedBank", &e.to_string()),
    };

    match route {
        StorageRoute::Local(local) => {
            let store = local.object_store();
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
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        }
        StorageRoute::Proxy(target) => {
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            proxy_s3_request(
                reqwest::Method::GET,
                &target.endpoint,
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
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub marker: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    #[serde(rename = "seed-bank")]
    pub storage: Option<String>,
}

/// List objects in a bucket
pub async fn list_objects(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectsQuery>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }

    let selector = SeedBankSelector {
        seed_bank: query.storage.clone(),
    };

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let route = match StorageRoute::for_read(&selected, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(route) => route,
        Err(e) => return xml_error(StatusCode::SERVICE_UNAVAILABLE, "NoSeedBank", &e.to_string()),
    };

    let max_keys = query.max_keys.unwrap_or(DEFAULT_MAX_KEYS).min(MAX_MAX_KEYS);

    match route {
        StorageRoute::Local(local) => {
            let store = local.object_store();
            match store
                .list_objects(
                    &bucket,
                    query.prefix.as_deref(),
                    query.delimiter.as_deref(),
                    query.marker.as_deref(),
                    max_keys,
                )
                .await
            {
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
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        }
        StorageRoute::Proxy(target) => {
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
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query_params.push(("seed-bank".to_string(), selected));
            }

            proxy_s3_request(
                reqwest::Method::GET,
                &target.endpoint,
                &format!("/api/v1/storage/s3/{}", bucket),
                query_params,
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// XML builders
// ============================================================================

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
        xml.push_str(&format!(
            "\n  <Delimiter>{}</Delimiter>",
            escape_xml(delimiter)
        ));
    }

    xml.push_str(&format!(
        "\n  <IsTruncated>{}</IsTruncated>",
        result.is_truncated
    ));

    for obj in &result.contents {
        xml.push_str("\n  <Contents>");
        xml.push_str(&format!("\n    <Key>{}</Key>", escape_xml(&obj.key)));
        xml.push_str(&format!(
            "\n    <LastModified>{}</LastModified>",
            escape_xml(&obj.last_modified)
        ));
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

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::storage::{ListResult, ObjectMetadata};

    // ── has_path_traversal ─────────────────────────────────────────────

    #[test]
    fn test_path_traversal_detects_parent_dir() {
        assert!(has_path_traversal("../etc/passwd"));
        assert!(has_path_traversal("foo/../bar"));
    }

    #[test]
    fn test_path_traversal_detects_backslash() {
        assert!(has_path_traversal("foo\\bar"));
        assert!(has_path_traversal("..\\secret"));
    }

    #[test]
    fn test_path_traversal_clean_paths_pass() {
        assert!(!has_path_traversal("mybucket"));
        assert!(!has_path_traversal("photos/2026/jan"));
        assert!(!has_path_traversal("file.txt"));
    }

    #[test]
    fn test_path_traversal_dot_alone_is_ok() {
        // Single dot (current dir) is not a traversal risk for buckets/keys
        // since we join it under a mount path. The function checks ParentDir,
        // RootDir, and Prefix — CurDir (".") is allowed.
        assert!(!has_path_traversal("./file.txt"));
    }

    // ── validate_bucket ────────────────────────────────────────────────

    #[test]
    fn test_validate_bucket_empty_returns_error() {
        let resp = validate_bucket("");
        assert!(resp.is_some());
    }

    #[test]
    fn test_validate_bucket_traversal_returns_error() {
        let resp = validate_bucket("../etc");
        assert!(resp.is_some());
    }

    #[test]
    fn test_validate_bucket_valid_returns_none() {
        assert!(validate_bucket("my-bucket").is_none());
        assert!(validate_bucket("photos").is_none());
        assert!(validate_bucket("data-2026").is_none());
    }

    // ── validate_key ───────────────────────────────────────────────────

    #[test]
    fn test_validate_key_empty_returns_error() {
        let resp = validate_key("");
        assert!(resp.is_some());
    }

    #[test]
    fn test_validate_key_traversal_returns_error() {
        let resp = validate_key("../../shadow");
        assert!(resp.is_some());
    }

    #[test]
    fn test_validate_key_valid_returns_none() {
        assert!(validate_key("report.pdf").is_none());
        assert!(validate_key("logs/2026/jan.log").is_none());
    }

    // ── escape_xml ─────────────────────────────────────────────────────

    #[test]
    fn test_escape_xml_ampersand() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
    }

    #[test]
    fn test_escape_xml_angle_brackets() {
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn test_escape_xml_quotes() {
        assert_eq!(escape_xml(r#"he said "hi""#), "he said &quot;hi&quot;");
        assert_eq!(escape_xml("it's"), "it&apos;s");
    }

    #[test]
    fn test_escape_xml_no_special_chars() {
        assert_eq!(escape_xml("plain text"), "plain text");
    }

    #[test]
    fn test_escape_xml_all_special_chars() {
        let input = "<>&\"'";
        let expected = "&lt;&gt;&amp;&quot;&apos;";
        assert_eq!(escape_xml(input), expected);
    }

    // ── build_list_all_buckets_result ───────────────────────────────────

    #[test]
    fn test_list_all_buckets_empty() {
        let xml = build_list_all_buckets_result(&[]);
        assert!(xml.contains("<Buckets>"));
        assert!(xml.contains("</Buckets>"));
        assert!(!xml.contains("<Bucket>"));
    }

    #[test]
    fn test_list_all_buckets_includes_names() {
        let buckets = vec!["photos".to_string(), "backups".to_string()];
        let xml = build_list_all_buckets_result(&buckets);
        assert!(xml.contains("<Name>photos</Name>"));
        assert!(xml.contains("<Name>backups</Name>"));
        assert_eq!(xml.matches("<Bucket>").count(), 2);
    }

    #[test]
    fn test_list_all_buckets_escapes_names() {
        let buckets = vec!["my<bucket>".to_string()];
        let xml = build_list_all_buckets_result(&buckets);
        assert!(xml.contains("<Name>my&lt;bucket&gt;</Name>"));
    }

    #[test]
    fn test_list_all_buckets_has_owner() {
        let xml = build_list_all_buckets_result(&[]);
        assert!(xml.contains("<ID>zen-garden</ID>"));
        assert!(xml.contains("<DisplayName>zen-garden</DisplayName>"));
    }

    // ── build_list_bucket_result ───────────────────────────────────────

    #[test]
    fn test_list_bucket_result_empty() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = build_list_bucket_result("test-bucket", "", "", 1000, "", &result);
        assert!(xml.contains("<Name>test-bucket</Name>"));
        assert!(xml.contains("<IsTruncated>false</IsTruncated>"));
        assert!(!xml.contains("<Contents>"));
    }

    #[test]
    fn test_list_bucket_result_with_objects() {
        let result = ListResult {
            contents: vec![ObjectMetadata {
                key: "file.txt".to_string(),
                size: 1024,
                last_modified: "2026-01-01T00:00:00Z".to_string(),
                etag: "\"abc123\"".to_string(),
                content_type: "text/plain".to_string(),
            }],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = build_list_bucket_result("data", "", "", 1000, "", &result);
        assert!(xml.contains("<Key>file.txt</Key>"));
        assert!(xml.contains("<Size>1024</Size>"));
        assert!(xml.contains("<StorageClass>STANDARD</StorageClass>"));
    }

    #[test]
    fn test_list_bucket_result_with_common_prefixes() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec!["logs/".to_string(), "data/".to_string()],
            is_truncated: false,
            next_marker: None,
        };
        let xml = build_list_bucket_result("mybucket", "", "", 1000, "/", &result);
        assert!(xml.contains("<Prefix>logs/</Prefix>"));
        assert!(xml.contains("<Prefix>data/</Prefix>"));
        assert!(xml.contains("<Delimiter>/</Delimiter>"));
    }

    #[test]
    fn test_list_bucket_result_truncated() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: true,
            next_marker: Some("marker123".to_string()),
        };
        let xml = build_list_bucket_result("bucket", "", "", 10, "", &result);
        assert!(xml.contains("<IsTruncated>true</IsTruncated>"));
        assert!(xml.contains("<MaxKeys>10</MaxKeys>"));
    }

    // ── get_storage_name ───────────────────────────────────────────────

    #[test]
    fn test_get_storage_name_from_query() {
        let headers = HeaderMap::new();
        let selector = SeedBankSelector {
            seed_bank: Some("my-bank".to_string()),
        };
        assert_eq!(
            get_storage_name(&headers, &selector),
            Some("my-bank".to_string())
        );
    }

    #[test]
    fn test_get_storage_name_header_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_SEED_BANK, "header-bank".parse().unwrap());
        let selector = SeedBankSelector {
            seed_bank: Some("query-bank".to_string()),
        };
        assert_eq!(
            get_storage_name(&headers, &selector),
            Some("header-bank".to_string())
        );
    }

    #[test]
    fn test_get_storage_name_none_when_empty() {
        let headers = HeaderMap::new();
        let selector = SeedBankSelector { seed_bank: None };
        assert_eq!(get_storage_name(&headers, &selector), None);
    }

    #[test]
    fn test_get_storage_name_empty_header_falls_through() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_SEED_BANK, "  ".parse().unwrap());
        let selector = SeedBankSelector {
            seed_bank: Some("fallback".to_string()),
        };
        assert_eq!(
            get_storage_name(&headers, &selector),
            Some("fallback".to_string())
        );
    }
}
