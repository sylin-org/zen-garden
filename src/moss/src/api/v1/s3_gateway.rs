//! S3-compatible Object Storage Gateway API
//!
//! Provides S3-compatible endpoints for storing and retrieving objects from seed banks.
//! See docs/reference/S3-API-REFERENCE.md and docs/decisions/STORAGE-0002-api-structure.md.
//!
//! ## Endpoints
//!
//! ```text
//! GET  /api/v1/stone/storage/s3              → List buckets (XML)
//! GET  /api/v1/stone/storage/s3/:bucket      → List objects in bucket (XML)
//! PUT  /api/v1/stone/storage/s3/:bucket/*key → Put object
//! GET  /api/v1/stone/storage/s3/:bucket/*key → Get object (raw bytes)
//! HEAD /api/v1/stone/storage/s3/:bucket/*key → Object metadata (headers)
//! DELETE /api/v1/stone/storage/s3/:bucket/*key → Delete object
//! ```
//!
//! ## Headers
//!
//! - `X-App-Name` - Required. Application namespace for isolation.
//! - `Content-Type` - MIME type for PUT (default: application/octet-stream)
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

// ============================================================================
// Constants
// ============================================================================

const HEADER_APP_NAME: &str = "x-app-name";
const DEFAULT_MAX_KEYS: usize = 1000;
const MAX_MAX_KEYS: usize = 1000;

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract app name from headers
fn get_app_name(headers: &HeaderMap) -> Option<String> {
    headers
        .get(HEADER_APP_NAME)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Find the default seed bank for storage operations
async fn get_default_seed_bank() -> Result<String, (StatusCode, String)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to scan seed banks: {}", e)))?;
    
    let banks = registry.list();
    
    // Prefer local (non-roaming) seed banks that are online
    let local_bank = banks.iter()
        .find(|b| !b.roaming && b.online);
    
    if let Some(bank) = local_bank {
        return Ok(bank.mount_path.clone());
    }
    
    // Fall back to any available seed bank
    banks.first()
        .map(|b| b.mount_path.clone())
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "No seed banks available".to_string()))
}

/// Build XML error response
fn xml_error(status: StatusCode, code: &str, message: &str) -> Response {
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
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

// ============================================================================
// PUT /api/v1/stone/storage/s3/:bucket/*key - Put Object
// ============================================================================

/// Put an object to seed bank storage
pub async fn put_object(
    State(_state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => return xml_error(StatusCode::BAD_REQUEST, "MissingAppName", "X-App-Name header is required"),
    };

    // Validate bucket and key
    if bucket.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket name cannot be empty");
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", "Object key cannot be empty");
    }

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    // Get content type
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    // Store object
    let store = ObjectStore::new(&mount_path);
    match store.put_object(&app, &bucket, &key, content_type, &body).await {
        Ok(result) => {
            debug!(app = %app, bucket = %bucket, key = %key, size = body.len(), "PUT object success");
            
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

// ============================================================================
// GET /api/v1/stone/storage/s3/:bucket/*key - Get Object
// ============================================================================

/// Get an object from seed bank storage
pub async fn get_object(
    State(_state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => return xml_error(StatusCode::BAD_REQUEST, "MissingAppName", "X-App-Name header is required"),
    };

    // Validate bucket and key
    if bucket.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket name cannot be empty");
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "Object key cannot be empty");
    }

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    // Retrieve object
    let store = ObjectStore::new(&mount_path);
    match store.get_object(&app, &bucket, &key).await {
        Ok(Some((data, meta))) => {
            debug!(app = %app, bucket = %bucket, key = %key, size = data.len(), "GET object success");
            
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
            xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &format!("Key '{}' not found", key))
        }
        Err(e) => {
            warn!(error = %e, "GET object failed");
            xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
        }
    }
}

// ============================================================================
// HEAD /api/v1/stone/storage/s3/:bucket/*key - Head Object
// ============================================================================

/// Get object metadata without body
pub async fn head_object(
    State(_state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("".into())
                .unwrap();
        }
    };

    // Validate bucket and key
    if bucket.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("".into())
            .unwrap();
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("".into())
            .unwrap();
    }

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, _)) => {
            return Response::builder()
                .status(status)
                .body("".into())
                .unwrap();
        }
    };

    // Get metadata
    let store = ObjectStore::new(&mount_path);
    match store.head_object(&app, &bucket, &key).await {
        Ok(Some(meta)) => {
            debug!(app = %app, bucket = %bucket, key = %key, "HEAD object success");
            
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, &meta.content_type)
                .header(header::CONTENT_LENGTH, meta.size)
                .header(header::ETAG, &meta.etag)
                .header(header::LAST_MODIFIED, &meta.last_modified)
                .body("".into())
                .unwrap()
        }
        Ok(None) => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("".into())
                .unwrap()
        }
        Err(_) => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("".into())
                .unwrap()
        }
    }
}

// ============================================================================
// DELETE /api/v1/stone/storage/s3/:bucket/*key - Delete Object
// ============================================================================

/// Delete an object from seed bank storage
pub async fn delete_object(
    State(_state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => return xml_error(StatusCode::BAD_REQUEST, "MissingAppName", "X-App-Name header is required"),
    };

    // Validate bucket and key
    if bucket.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket name cannot be empty");
    }
    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", "Object key cannot be empty");
    }

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    // Delete object
    let store = ObjectStore::new(&mount_path);
    match store.delete_object(&app, &bucket, &key).await {
        Ok(_) => {
            debug!(app = %app, bucket = %bucket, key = %key, "DELETE object success");
            
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body("".into())
                .unwrap()
        }
        Err(e) => {
            warn!(error = %e, "DELETE object failed");
            xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string())
        }
    }
}

// ============================================================================
// GET /api/v1/stone/storage/s3 - List Buckets
// ============================================================================

/// List all buckets (grouped by app namespace)
pub async fn list_buckets(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => return xml_error(StatusCode::BAD_REQUEST, "MissingAppName", "X-App-Name header is required"),
    };

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    // List buckets for this app
    let store = ObjectStore::new(&mount_path);
    match store.list_buckets(&app).await {
        Ok(buckets) => {
            debug!(app = %app, count = buckets.len(), "LIST buckets success");
            
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

// ============================================================================
// GET /api/v1/stone/storage/s3/:bucket - List Objects
// ============================================================================

/// Query parameters for list objects
#[derive(Debug, Deserialize)]
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
}

/// List objects in a bucket
pub async fn list_objects(
    State(_state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectsQuery>,
    headers: HeaderMap,
) -> Response {
    // Get app name from header
    let app = match get_app_name(&headers) {
        Some(a) => a,
        None => return xml_error(StatusCode::BAD_REQUEST, "MissingAppName", "X-App-Name header is required"),
    };

    // Validate bucket
    if bucket.is_empty() {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidBucket", "Bucket name cannot be empty");
    }

    // Get default seed bank
    let mount_path = match get_default_seed_bank().await {
        Ok(p) => p,
        Err((status, msg)) => return xml_error(status, "NoSeedBank", &msg),
    };

    let max_keys = query.max_keys.unwrap_or(DEFAULT_MAX_KEYS).min(MAX_MAX_KEYS);

    // List objects
    let store = ObjectStore::new(&mount_path);
    match store.list_objects(
        &app,
        &bucket,
        query.prefix.as_deref(),
        query.delimiter.as_deref(),
        query.marker.as_deref(),
        max_keys,
    ).await {
        Ok(result) => {
            debug!(
                app = %app,
                bucket = %bucket,
                count = result.contents.len(),
                truncated = result.is_truncated,
                "LIST objects success"
            );
            
            // Build XML response
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
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
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
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
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
