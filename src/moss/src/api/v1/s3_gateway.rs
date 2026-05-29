//! S3-compatible Object Storage Gateway API (STORAGE-0009 / STORAGE-0016)
//!
//! Provides S3-compatible endpoints for storing and retrieving objects.
//! Uses `StorageService` for resolution and routing.
//!
//! STORAGE-0016: Unified namespace — objects live at mount root alongside native files.
//! Metadata sidecars under `.zen-garden/meta/`. Range reads via HTTP Range header.
//!
//! ## Endpoints
//!
//! ```text
//! GET    /api/v1/storage/s3                  → List buckets (XML)
//! PUT    /api/v1/storage/s3/:bucket          → Create bucket
//! GET    /api/v1/storage/s3/:bucket          → List objects (V1/V2 XML)
//! PUT    /api/v1/storage/s3/:bucket/*key     → Put object
//! GET    /api/v1/storage/s3/:bucket/*key     → Get object (raw bytes, Range supported)
//! HEAD   /api/v1/storage/s3/:bucket/*key     → Object metadata (headers)
//! DELETE /api/v1/storage/s3/:bucket/*key     → Delete object
//! PUT    /api/v1/storage/s3/:bucket/*key     → Copy object (x-amz-copy-source header)
//! ```
//!
//! ## Headers
//!
//! - `X-Seed-Bank` - Optional. Select a specific storage by name.
//! - `Content-Type` - MIME type for PUT (default: application/octet-stream)
//! - `Range` - Optional. Byte range for GET (e.g., `bytes=0-99`)
//! - `x-amz-copy-source` - Copy source in PUT (e.g., `/source-bucket/source-key`)
//!
//! ## Query Params
//!
//! - `seed-bank` - Optional. Select a specific storage by name.
//! - `list-type=2` - Use ListObjectsV2 (continuation-token based pagination)

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use tracing::{debug, warn};

use super::s3_xml::{self, to_s3_xml};

use crate::Moss;
use crate::infra::storage::handle::StorageResolver;
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
    pub seed_bank: Option<String>,
}

impl SeedBankSelector {
    fn name(&self) -> Option<&str> {
        self.seed_bank.as_deref()
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
    selector.name().map(|s| s.to_string())
}

fn has_path_traversal(value: &str) -> bool {
    garden_common::constants::storage::share::has_path_traversal(value)
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

/// Parse HTTP Range header, returning `(start, optional_end)`.
///
/// Supports only the common `bytes=N-M` and `bytes=N-` forms.
/// Returns `None` if the header is absent or unparseable.
fn parse_range_header(headers: &HeaderMap) -> Option<(u64, Option<u64>)> {
    let value = headers.get(header::RANGE)?.to_str().ok()?;
    let range = value.strip_prefix("bytes=")?;
    let (start_str, end_str) = range.split_once('-')?;
    let start = start_str.trim().parse::<u64>().ok()?;
    let end = if end_str.trim().is_empty() {
        None
    } else {
        Some(end_str.trim().parse::<u64>().ok()?)
    };
    Some((start, end))
}

/// Result of evaluating conditional request headers against object metadata.
enum ConditionalResult {
    /// Proceed with the request normally.
    Proceed,
    /// Return 304 Not Modified (GET/HEAD with If-None-Match or If-Modified-Since).
    NotModified,
    /// Return 412 Precondition Failed (If-Match or If-Unmodified-Since failed).
    PreconditionFailed,
}

/// Evaluate S3 conditional request headers against object metadata.
///
/// Implements the S3/HTTP precedence:
/// 1. If-Match → 412 if ETag doesn't match
/// 2. If-Unmodified-Since → 412 if modified after date
/// 3. If-None-Match → 304 if ETag matches
/// 4. If-Modified-Since → 304 if not modified since date
fn evaluate_conditionals(
    headers: &HeaderMap,
    etag: &str,
    last_modified: &str,
) -> ConditionalResult {
    // If-Match: proceed only if ETag matches
    if let Some(val) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok())
        && !etag_matches(val, etag)
    {
        return ConditionalResult::PreconditionFailed;
    }

    // If-Unmodified-Since: proceed only if not modified after date
    if let Some(val) = headers
        .get(header::IF_UNMODIFIED_SINCE)
        .and_then(|v| v.to_str().ok())
        && let Ok(since) = chrono::DateTime::parse_from_rfc2822(val)
        && let Ok(modified) = chrono::DateTime::parse_from_rfc3339(last_modified)
        && modified > since
    {
        return ConditionalResult::PreconditionFailed;
    }

    // If-None-Match: 304 if ETag matches
    if let Some(val) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        && etag_matches(val, etag)
    {
        return ConditionalResult::NotModified;
    }

    // If-Modified-Since: 304 if not modified since date
    if let Some(val) = headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|v| v.to_str().ok())
        && let Ok(since) = chrono::DateTime::parse_from_rfc2822(val)
        && let Ok(modified) = chrono::DateTime::parse_from_rfc3339(last_modified)
        && modified <= since
    {
        return ConditionalResult::NotModified;
    }

    ConditionalResult::Proceed
}

/// Check if an ETag matches a conditional header value.
/// Supports `*` (match any) and comma-separated lists.
fn etag_matches(header_value: &str, etag: &str) -> bool {
    let trimmed = header_value.trim();
    if trimmed == "*" {
        return true;
    }
    trimmed
        .split(',')
        .any(|v| v.trim().trim_matches('"') == etag.trim_matches('"'))
}

/// Extract x-amz-meta-* custom metadata headers into a map.
/// Header names are lowercased and the "x-amz-meta-" prefix is stripped.
fn extract_custom_metadata(headers: &HeaderMap) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str();
        if let Some(stripped) = key.strip_prefix("x-amz-meta-")
            && let Ok(val) = value.to_str()
        {
            meta.insert(stripped.to_string(), val.to_string());
        }
    }
    meta
}

/// Validate presigned token if present in query params.
///
/// If `X-Moss-Token` and `X-Moss-Expires` are present, validates the token.
/// Returns `Some(error_response)` if validation fails, `None` if valid or no token present.
async fn check_presign_token(
    state: &Moss,
    method: &str,
    bucket: &str,
    key: &str,
    query_string: &str,
) -> Option<Response> {
    // Parse X-Moss-Token and X-Moss-Expires from query string
    let mut token: Option<String> = None;
    let mut expires: Option<i64> = None;

    for pair in query_string.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "X-Moss-Token" => {
                    token = Some(urlencoding::decode(v).unwrap_or_default().to_string())
                }
                "X-Moss-Expires" => expires = v.parse().ok(),
                _ => {}
            }
        }
    }

    // If no presign params, allow (unsigned mode for regular requests)
    let (token, expires_ts) = match (token, expires) {
        (Some(t), Some(e)) => (t, e),
        _ => return None, // No presign token — pass through
    };

    // Token present — MUST validate
    let secret = super::s3_presign::derive_presign_secret(state).await;
    match super::s3_presign::validate_presign_token(
        &secret, method, bucket, key, &token, expires_ts,
    ) {
        Ok(()) => None, // Valid
        Err(reason) => {
            warn!(bucket = %bucket, key = %key, reason, "Presigned token validation failed");
            Some(xml_error(StatusCode::FORBIDDEN, "AccessDenied", reason))
        }
    }
}

/// Safely build an HTTP response, returning an XML error on builder failure.
///
/// `Response::builder().body()` can fail if invalid header values were inserted
/// (e.g., non-ASCII custom metadata). This wrapper prevents panics from `.unwrap()`.
fn build_response(
    builder: axum::http::response::Builder,
    body: impl Into<axum::body::Body>,
) -> Response {
    builder.body(body.into()).unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to build HTTP response");
        // Minimal fallback — avoids recursion through xml_error → build_response
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(header::CONTENT_TYPE, "text/plain")
            .body("Internal Server Error".into())
            .expect("minimal error response cannot fail")
    })
}

/// Build XML error response
fn xml_error(status: StatusCode, code: &str, message: &str) -> Response {
    let body = to_s3_xml(&s3_xml::S3Error {
        code: code.to_string(),
        message: message.to_string(),
    });

    build_response(
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/xml"),
        body,
    )
}

async fn proxy_s3_request(
    method: reqwest::Method,
    endpoint: &str,
    path: &str,
    query: Vec<(String, String)>,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    static S3_PROXY_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("S3 proxy HTTP client")
    });
    let client = &*S3_PROXY_CLIENT;
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

    build_response(builder, body)
}

// ============================================================================
// PUT /api/v1/storage/s3/:bucket/*key - Put Object
// ============================================================================

/// Put an object to storage (or copy, or multipart part upload)
pub async fn put_object(
    State(state): State<Moss>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<UploadPartQuery>,
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

    // Detect multipart UploadPart: PUT with partNumber + uploadId
    if query.part_number.is_some() && query.upload_id.is_some() {
        return upload_part(
            State(state),
            Path((bucket.clone(), key.to_string())),
            Query(query),
            headers,
            body,
        )
        .await;
    }

    let selector = SeedBankSelector {
        seed_bank: query.seed_bank,
    };

    // Detect CopyObject: PUT with x-amz-copy-source header
    if let Some(copy_source) = headers
        .get(HEADER_COPY_SOURCE)
        .and_then(|v| v.to_str().ok())
    {
        return copy_object(&state, &bucket, key, copy_source, &headers, &selector).await;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: Some(state.current.storage.coordination.tick.raw.clone()),
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_write() {
        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream");

        let custom_meta = extract_custom_metadata(&headers);
        match store
            .put_object_with_metadata(&bucket, key, content_type, &body, custom_meta)
            .await
        {
            Ok(result) => {
                debug!(storage = %handle.storage_name(), bucket = %bucket, key = %key, size = body.len(), "PUT object success");
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
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
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

// ============================================================================
// GET /api/v1/storage/s3/:bucket/*key - Get Object
// ============================================================================

/// Get an object from storage
pub async fn get_object(
    State(state): State<Moss>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
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

    // Validate presigned token if present
    if let Some(resp) = check_presign_token(
        &state,
        "GET",
        &bucket,
        key,
        raw_query.as_deref().unwrap_or(""),
    )
    .await
    {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_read(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_read() {
        // Check bucket existence — return NoSuchBucket (not NoSuchKey) for missing buckets
        if !store.bucket_exists(&bucket) {
            return xml_error(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                &format!("Bucket '{}' does not exist", bucket),
            );
        }

        // Parse optional Range header (e.g., "bytes=0-99")
        let range = parse_range_header(&headers);

        // Evaluate conditionals before reading data (avoids unnecessary I/O)
        if let Ok(Some(meta)) = store.head_object(&bucket, key).await {
            match evaluate_conditionals(&headers, &meta.etag, &meta.last_modified) {
                ConditionalResult::NotModified => {
                    return Response::builder()
                        .status(StatusCode::NOT_MODIFIED)
                        .header(header::ETAG, &meta.etag)
                        .header(header::LAST_MODIFIED, &meta.last_modified)
                        .body("".into())
                        .unwrap();
                }
                ConditionalResult::PreconditionFailed => {
                    return xml_error(
                        StatusCode::PRECONDITION_FAILED,
                        "PreconditionFailed",
                        "Conditional request failed",
                    );
                }
                ConditionalResult::Proceed => {}
            }
        }

        if let Some((range_start, range_end)) = range {
            // Ranged read — return HTTP 206 Partial Content
            match store.head_object(&bucket, key).await {
                Ok(Some(meta)) => {
                    let total_size = meta.size;
                    let end = range_end
                        .unwrap_or(total_size.saturating_sub(1))
                        .min(total_size.saturating_sub(1));
                    let start = range_start.min(end);
                    let length = end - start + 1;

                    match store.get_object_range(&bucket, key, start, length).await {
                        Ok(Some((data, _total, _meta))) => {
                            debug!(storage = %handle.storage_name(), bucket = %bucket, key = %key, start, end, length, "GET object range success");
                            Response::builder()
                                .status(StatusCode::PARTIAL_CONTENT)
                                .header(header::CONTENT_TYPE, &meta.content_type)
                                .header(header::CONTENT_LENGTH, data.len())
                                .header(header::ETAG, &meta.etag)
                                .header(header::LAST_MODIFIED, &meta.last_modified)
                                .header(
                                    "Content-Range",
                                    format!("bytes {}-{}/{}", start, end, total_size),
                                )
                                .header(header::ACCEPT_RANGES, "bytes")
                                .body(data.into())
                                .unwrap()
                        }
                        Ok(None) => xml_error(
                            StatusCode::NOT_FOUND,
                            "NoSuchKey",
                            &format!("Key '{}' not found", key),
                        ),
                        Err(e) => {
                            warn!(error = %e, "GET object range failed");
                            xml_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "InternalError",
                                &e.to_string(),
                            )
                        }
                    }
                }
                Ok(None) => xml_error(
                    StatusCode::NOT_FOUND,
                    "NoSuchKey",
                    &format!("Key '{}' not found", key),
                ),
                Err(e) => {
                    warn!(error = %e, "GET object head for range failed");
                    xml_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &e.to_string(),
                    )
                }
            }
        } else {
            // Full read
            match store.get_object(&bucket, key).await {
                Ok(Some((data, meta))) => {
                    debug!(storage = %handle.storage_name(), bucket = %bucket, key = %key, size = data.len(), "GET object success");
                    let mut builder = Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, &meta.content_type)
                        .header(header::CONTENT_LENGTH, data.len())
                        .header(header::ETAG, &meta.etag)
                        .header(header::LAST_MODIFIED, &meta.last_modified)
                        .header(header::ACCEPT_RANGES, "bytes");
                    for (k, v) in &meta.custom_metadata {
                        if let Ok(val) = axum::http::HeaderValue::from_str(v) {
                            builder = builder.header(format!("x-amz-meta-{}", k), val);
                        }
                    }
                    build_response(builder, data)
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
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
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

// ============================================================================
// HEAD /api/v1/storage/s3/:bucket/*key - Head Object
// ============================================================================

/// Get object metadata without body
pub async fn head_object(
    State(state): State<Moss>,
    Path((bucket, key)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
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

    // Validate presigned token if present
    if let Some(resp) = check_presign_token(
        &state,
        "HEAD",
        &bucket,
        key,
        raw_query.as_deref().unwrap_or(""),
    )
    .await
    {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_read(&selected).await {
        Ok(handle) => handle,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body("".into())
                .unwrap();
        }
    };

    if let Some(store) = handle.object_store_for_read() {
        if !store.bucket_exists(&bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("".into())
                .unwrap();
        }

        match store.head_object(&bucket, key).await {
            Ok(Some(meta)) => {
                // Evaluate conditional headers
                match evaluate_conditionals(&headers, &meta.etag, &meta.last_modified) {
                    ConditionalResult::NotModified => {
                        return Response::builder()
                            .status(StatusCode::NOT_MODIFIED)
                            .header(header::ETAG, &meta.etag)
                            .header(header::LAST_MODIFIED, &meta.last_modified)
                            .body("".into())
                            .unwrap();
                    }
                    ConditionalResult::PreconditionFailed => {
                        return Response::builder()
                            .status(StatusCode::PRECONDITION_FAILED)
                            .body("".into())
                            .unwrap();
                    }
                    ConditionalResult::Proceed => {}
                }

                debug!(storage = %handle.storage_name(), bucket = %bucket, key = %key, "HEAD object success");
                let mut builder = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, &meta.content_type)
                    .header(header::CONTENT_LENGTH, meta.size)
                    .header(header::ETAG, &meta.etag)
                    .header(header::LAST_MODIFIED, &meta.last_modified)
                    .header(header::ACCEPT_RANGES, "bytes");
                for (k, v) in &meta.custom_metadata {
                    if let Ok(val) = axum::http::HeaderValue::from_str(v) {
                        builder = builder.header(format!("x-amz-meta-{}", k), val);
                    }
                }
                build_response(builder, "")
            }
            Ok(None) => build_response(Response::builder().status(StatusCode::NOT_FOUND), ""),
            Err(_) => build_response(
                Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR),
                "",
            ),
        }
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
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

// ============================================================================
// DELETE /api/v1/storage/s3/:bucket/*key - Delete Object
// ============================================================================

/// Query params for DELETE that optionally includes uploadId for multipart abort
#[derive(Debug, Default, Deserialize)]
pub struct DeleteObjectQuery {
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    #[serde(rename = "seed-bank")]
    pub seed_bank: Option<String>,
}

/// Delete an object from storage (or abort multipart upload if uploadId is present)
pub async fn delete_object(
    State(state): State<Moss>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<DeleteObjectQuery>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    let key = key.trim_start_matches('/');
    if let Some(resp) = validate_key(key) {
        return resp;
    }

    // Detect multipart abort: DELETE with uploadId
    if let Some(upload_id) = &query.upload_id {
        let selector = SeedBankSelector {
            seed_bank: query.seed_bank.clone(),
        };
        return abort_multipart_upload(&state, &bucket, key, upload_id, &headers, &selector).await;
    }

    let selector = SeedBankSelector {
        seed_bank: query.seed_bank,
    };
    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: Some(state.current.storage.coordination.tick.raw.clone()),
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_write() {
        match store.delete_object(&bucket, key).await {
            Ok(_) => {
                debug!(storage = %handle.storage_name(), bucket = %bucket, key = %key, "DELETE object success");
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
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
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

// ============================================================================
// GET /api/v1/storage/s3 - List Buckets
// ============================================================================

/// List all buckets
pub async fn list_buckets(
    State(state): State<Moss>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_read(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_read() {
        match store.list_buckets().await {
            Ok(buckets) => {
                debug!(storage = %handle.storage_name(), count = buckets.len(), "LIST buckets success");
                let xml = to_s3_xml(&s3_xml::ListAllMyBucketsResult::new(&buckets));
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
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
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

// ============================================================================
// GET /api/v1/storage/s3/:bucket - List Objects
// ============================================================================

/// Query parameters for list objects (supports V1 marker and V2 continuation-token)
#[derive(Debug, Default, Deserialize)]
pub struct ListObjectsQuery {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    /// V1 pagination marker
    pub marker: Option<String>,
    /// V2: `list-type=2` enables continuation-token based pagination
    #[serde(rename = "list-type")]
    pub list_type: Option<u8>,
    /// V2 continuation token (opaque, currently base64-encoded last key)
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    /// V2 start-after: start listing after this key
    #[serde(rename = "start-after")]
    pub start_after: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    #[serde(rename = "seed-bank")]
    pub storage: Option<String>,
}

/// List objects in a bucket (supports V1 and V2)
pub async fn list_objects(
    State(state): State<Moss>,
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

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_read(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let max_keys = query.max_keys.unwrap_or(DEFAULT_MAX_KEYS).min(MAX_MAX_KEYS);
    let is_v2 = query.list_type == Some(2);

    // For V2, decode continuation-token (base64 of last key) or use start-after
    let effective_marker = if is_v2 {
        query
            .continuation_token
            .as_ref()
            .and_then(|ct| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(ct)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            })
            .or_else(|| query.start_after.clone())
    } else {
        query.marker.clone()
    };

    if let Some(store) = handle.object_store_for_read() {
        match store
            .list_objects(
                &bucket,
                query.prefix.as_deref(),
                query.delimiter.as_deref(),
                effective_marker.as_deref(),
                max_keys,
            )
            .await
        {
            Ok(result) => {
                debug!(storage = %handle.storage_name(), bucket = %bucket, count = result.contents.len(), truncated = result.is_truncated, v2 = is_v2, "LIST objects success");

                let xml = if is_v2 {
                    to_s3_xml(&s3_xml::ListBucketResultV2::from_list_result(
                        &bucket,
                        query.prefix.as_deref().unwrap_or(""),
                        query.start_after.as_deref().unwrap_or(""),
                        query.continuation_token.as_deref(),
                        max_keys,
                        query.delimiter.as_deref().unwrap_or(""),
                        &result,
                    ))
                } else {
                    to_s3_xml(&s3_xml::ListBucketResult::from_list_result(
                        &bucket,
                        query.prefix.as_deref().unwrap_or(""),
                        query.marker.as_deref().unwrap_or(""),
                        max_keys,
                        query.delimiter.as_deref().unwrap_or(""),
                        &result,
                    ))
                };

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
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
        let mut query_params = Vec::new();
        if is_v2 {
            query_params.push(("list-type".to_string(), "2".to_string()));
        }
        if let Some(prefix) = &query.prefix {
            query_params.push(("prefix".to_string(), prefix.clone()));
        }
        if let Some(delimiter) = &query.delimiter {
            query_params.push(("delimiter".to_string(), delimiter.clone()));
        }
        if let Some(marker) = &query.marker {
            query_params.push(("marker".to_string(), marker.clone()));
        }
        if let Some(ct) = &query.continuation_token {
            query_params.push(("continuation-token".to_string(), ct.clone()));
        }
        if let Some(sa) = &query.start_after {
            query_params.push(("start-after".to_string(), sa.clone()));
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

// ============================================================================
// XML builders (delegated to s3_xml module)
// ============================================================================

// ============================================================================
// PUT /api/v1/storage/s3/:bucket - Create Bucket
// ============================================================================

/// Create a bucket (directory at mount root)
pub async fn create_bucket(
    State(state): State<Moss>,
    Path(bucket): Path<String>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }

    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(handle) => handle,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    if let Some(store) = handle.object_store_for_write() {
        match store.create_bucket(&bucket).await {
            Ok(()) => {
                debug!(storage = %handle.storage_name(), bucket = %bucket, "CREATE bucket success");
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(String::new().into())
                    .unwrap()
            }
            Err(e) => {
                warn!(error = %e, "CREATE bucket failed");
                xml_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    &e.to_string(),
                )
            }
        }
    } else {
        let target = handle
            .proxy_target()
            .expect("invariant: handle is either local or remote; local path returned None");
        let mut query = Vec::new();
        if selected != DEFAULT_REPLICA_SET_DISPLAY {
            query.push(("seed-bank".to_string(), selected));
        }
        proxy_s3_request(
            reqwest::Method::PUT,
            &target.endpoint,
            &format!("/api/v1/storage/s3/{}", bucket),
            query,
            &headers,
            None,
        )
        .await
    }
}

// ============================================================================
// ============================================================================
// POST /{bucket}/{key}?uploads - Initiate Multipart Upload
// ============================================================================

/// Initiate a multipart upload. Returns upload ID in XML.
pub async fn initiate_multipart_upload(
    State(state): State<Moss>,
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

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let mount_path = match handle.mount_path() {
        Some(p) => p.to_path_buf(),
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Multipart uploads require local storage",
            );
        }
    };

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    let mp = crate::infra::storage::multipart::MultipartStore::new(&mount_path);
    match mp.initiate(&bucket, key, content_type).await {
        Ok(upload_id) => {
            let xml = to_s3_xml(&s3_xml::InitiateMultipartUploadResult {
                bucket: bucket.clone(),
                key: key.to_string(),
                upload_id,
            });
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(xml.into())
                .unwrap()
        }
        Err(e) => xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

// ============================================================================
// PUT /{bucket}/{key}?partNumber=N&uploadId=ID - Upload Part
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UploadPartQuery {
    #[serde(rename = "partNumber")]
    pub part_number: Option<u16>,
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    #[serde(rename = "seed-bank")]
    pub seed_bank: Option<String>,
}

/// Upload a single part of a multipart upload.
pub async fn upload_part(
    State(state): State<Moss>,
    Path((bucket, _key)): Path<(String, String)>,
    Query(query): Query<UploadPartQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (upload_id, part_number) = match (&query.upload_id, query.part_number) {
        (Some(id), Some(pn)) => (id.clone(), pn),
        _ => {
            return xml_error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "uploadId and partNumber required",
            );
        }
    };

    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }

    let selector = SeedBankSelector {
        seed_bank: query.seed_bank.clone(),
    };
    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let mount_path = match handle.mount_path() {
        Some(p) => p.to_path_buf(),
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Multipart uploads require local storage",
            );
        }
    };

    let mp = crate::infra::storage::multipart::MultipartStore::new(&mount_path);
    match mp.upload_part(&upload_id, part_number, &body).await {
        Ok(etag) => Response::builder()
            .status(StatusCode::OK)
            .header(header::ETAG, &etag)
            .body("".into())
            .unwrap(),
        Err(e) => xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

// ============================================================================
// POST /{bucket}/{key}?uploadId=ID - Complete Multipart Upload
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CompleteMultipartQuery {
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    pub uploads: Option<String>, // presence of "uploads" = initiate, not complete
    #[serde(rename = "seed-bank")]
    pub seed_bank: Option<String>,
}

/// Complete a multipart upload: assembles parts and writes the final object.
pub async fn complete_or_initiate_multipart(
    State(state): State<Moss>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<CompleteMultipartQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // POST with ?uploads → initiate; POST with ?uploadId=X → complete
    if query.uploads.is_some() {
        return initiate_multipart_upload(
            State(state),
            Path((bucket, key)),
            Query(SeedBankSelector {
                seed_bank: query.seed_bank,
            }),
            headers,
        )
        .await;
    }

    let upload_id = match &query.upload_id {
        Some(id) => id.clone(),
        None => {
            return xml_error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "uploadId required",
            );
        }
    };

    if let Some(resp) = validate_bucket(&bucket) {
        return resp;
    }
    // key from path is validated at entry; completion uses upload.key from manifest

    let selector = SeedBankSelector {
        seed_bank: query.seed_bank.clone(),
    };
    let selected = get_storage_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: Some(state.current.storage.coordination.tick.raw.clone()),
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let mount_path = match handle.mount_path() {
        Some(p) => p.to_path_buf(),
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Multipart uploads require local storage",
            );
        }
    };

    // Parse part list from XML body
    let body_str = String::from_utf8_lossy(&body);
    let part_numbers =
        match s3_xml::from_s3_xml::<s3_xml::CompleteMultipartUploadRequest>(&body_str) {
            Ok(req) => req
                .parts
                .into_iter()
                .map(|p| p.part_number)
                .collect::<Vec<_>>(),
            Err(_) => {
                return xml_error(
                    StatusCode::BAD_REQUEST,
                    "MalformedXML",
                    "Could not parse CompleteMultipartUpload XML",
                );
            }
        };

    if part_numbers.is_empty() {
        return xml_error(
            StatusCode::BAD_REQUEST,
            "MalformedXML",
            "No parts specified",
        );
    }

    // Admission control (STORAGE-0020): completion assembles every part into
    // a temp file on the destination bank's filesystem before the final
    // object write. Gate on that filesystem (not the data filesystem), since
    // a bank is usually a separate mount.
    if let crate::domain::Verdict::Deny { reason } = state
        .capacity
        .reserve_path(
            mount_path.to_string_lossy().to_string(),
            crate::domain::ReserveRequest::new(format!(
                "multipart completion for {bucket}"
            )),
        )
        .await
    {
        return xml_error(StatusCode::SERVICE_UNAVAILABLE, "InsufficientStorage", &reason);
    }

    let mp = crate::infra::storage::multipart::MultipartStore::new(&mount_path);
    let (assembled, upload) = match mp.complete(&upload_id, &part_numbers).await {
        Ok(result) => result,
        Err(e) => {
            return xml_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &e.to_string(),
            );
        }
    };

    // Write the assembled object through the normal put path (enters changelog)
    let store = match handle.object_store_for_write() {
        Some(s) => s,
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Storage not writable",
            );
        }
    };

    match store
        .put_object(
            &upload.bucket,
            &upload.key,
            &upload.content_type,
            &assembled,
        )
        .await
    {
        Ok(result) => {
            // Clean up multipart staging
            let _ = mp.cleanup(&upload_id).await;

            let xml = to_s3_xml(&s3_xml::CompleteMultipartUploadResult {
                bucket: upload.bucket.clone(),
                key: upload.key.clone(),
                etag: result.etag.clone(),
            });
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(xml.into())
                .unwrap()
        }
        Err(e) => xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

// ============================================================================
// DELETE /{bucket}/{key}?uploadId=ID - Abort Multipart Upload
// ============================================================================

/// Abort a multipart upload (called when delete has uploadId query param).
pub async fn abort_multipart_upload(
    state: &Moss,
    _bucket: &str,
    _key: &str,
    upload_id: &str,
    headers: &HeaderMap,
    selector: &SeedBankSelector,
) -> Response {
    let selected = get_storage_name(headers, selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    let handle = match resolver.for_write(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let mount_path = match handle.mount_path() {
        Some(p) => p.to_path_buf(),
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Multipart uploads require local storage",
            );
        }
    };

    let mp = crate::infra::storage::multipart::MultipartStore::new(&mount_path);
    match mp.abort(upload_id).await {
        Ok(()) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body("".into())
            .unwrap(),
        Err(e) => xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

// PUT /api/v1/storage/s3/:bucket/*key with x-amz-copy-source - Copy Object
// ============================================================================

/// Header name for S3 CopyObject source
const HEADER_COPY_SOURCE: &str = "x-amz-copy-source";

/// Copy object: PUT with `x-amz-copy-source` header.
///
/// The `put_object` handler delegates here when the copy header is present.
/// Source format: `/{source-bucket}/{source-key}` or `{source-bucket}/{source-key}`.
pub async fn copy_object(
    state: &Moss,
    dest_bucket: &str,
    dest_key: &str,
    copy_source: &str,
    headers: &HeaderMap,
    selector: &SeedBankSelector,
) -> Response {
    // Parse source: strip leading '/', split into bucket/key
    let source = copy_source.trim_start_matches('/');
    let (src_bucket, src_key) = match source.split_once('/') {
        Some((b, k)) if !b.is_empty() && !k.is_empty() => (b, k),
        _ => {
            return xml_error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "x-amz-copy-source must be /{bucket}/{key}",
            );
        }
    };

    if has_path_traversal(src_bucket) || has_path_traversal(src_key) {
        return xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "Copy source contains invalid path segments",
        );
    }

    let selected = get_storage_name(headers, selector)
        .unwrap_or_else(|| DEFAULT_REPLICA_SET_DISPLAY.to_string());

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };

    // Read from source
    let read_handle = match resolver.for_read(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let store = match read_handle.object_store_for_read() {
        Some(s) => s,
        None => {
            // Remote storage — proxy the entire COPY to the Primary stone
            let target = read_handle
                .proxy_target()
                .expect("invariant: handle is either local or remote; local path returned None");
            let mut query = Vec::new();
            if selected != DEFAULT_REPLICA_SET_DISPLAY {
                query.push(("seed-bank".to_string(), selected));
            }
            return proxy_s3_request(
                reqwest::Method::PUT,
                &target.endpoint,
                &format!("/api/v1/storage/s3/{}/{}", dest_bucket, dest_key),
                query,
                headers,
                None,
            )
            .await;
        }
    };

    let (data, src_meta) = match store.get_object(src_bucket, src_key).await {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            return xml_error(
                StatusCode::NOT_FOUND,
                "NoSuchKey",
                &format!(
                    "Source key '{}' not found in bucket '{}'",
                    src_key, src_bucket
                ),
            );
        }
        Err(e) => {
            warn!(error = %e, "COPY source read failed");
            return xml_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &e.to_string(),
            );
        }
    };

    // Write to destination
    let write_handle = match resolver.for_write(&selected).await {
        Ok(h) => h,
        Err(e) => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NoSeedBank",
                &e.to_string(),
            );
        }
    };

    let dest_store = match write_handle.object_store_for_write() {
        Some(s) => s,
        None => {
            return xml_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "NotLocal",
                "Copy destination must be local",
            );
        }
    };

    match dest_store
        .put_object(dest_bucket, dest_key, &src_meta.content_type, &data)
        .await
    {
        Ok(put_result) => {
            debug!(
                src_bucket = %src_bucket, src_key = %src_key,
                dest_bucket = %dest_bucket, dest_key = %dest_key,
                "COPY object success"
            );
            let xml = to_s3_xml(&s3_xml::CopyObjectResult {
                etag: put_result.etag.clone(),
                last_modified: src_meta.last_modified.clone(),
            });
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(xml.into())
                .unwrap()
        }
        Err(e) => {
            warn!(error = %e, "COPY destination write failed");
            xml_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &e.to_string(),
            )
        }
    }
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

    // ── s3_xml serialization ────────────────────────────────────────────

    #[test]
    fn test_xml_escapes_special_chars() {
        // quick-xml handles escaping automatically
        let xml = to_s3_xml(&s3_xml::S3Error {
            code: "Test".to_string(),
            message: "a<b>&c".to_string(),
        });
        assert!(xml.contains("a&lt;b&gt;&amp;c"));
    }

    // ── ListAllMyBucketsResult ──────────────────────────────────────────

    #[test]
    fn test_list_all_buckets_empty() {
        let xml = to_s3_xml(&s3_xml::ListAllMyBucketsResult::new(&[]));
        assert!(xml.contains("<Buckets"));
        assert!(!xml.contains("<Bucket>"));
    }

    #[test]
    fn test_list_all_buckets_includes_names() {
        let now = chrono::Utc::now();
        let buckets = vec![("photos".to_string(), now), ("backups".to_string(), now)];
        let xml = to_s3_xml(&s3_xml::ListAllMyBucketsResult::new(&buckets));
        assert!(xml.contains("<Name>photos</Name>"));
        assert!(xml.contains("<Name>backups</Name>"));
    }

    #[test]
    fn test_list_all_buckets_escapes_names() {
        let buckets = vec![("my<bucket>".to_string(), chrono::Utc::now())];
        let xml = to_s3_xml(&s3_xml::ListAllMyBucketsResult::new(&buckets));
        assert!(xml.contains("<Name>my&lt;bucket&gt;</Name>"));
    }

    #[test]
    fn test_list_all_buckets_has_owner() {
        let xml = to_s3_xml(&s3_xml::ListAllMyBucketsResult::new(&[]));
        assert!(xml.contains("<ID>zen-garden</ID>"));
        assert!(xml.contains("<DisplayName>zen-garden</DisplayName>"));
    }

    // ── ListBucketResult (V1) ───────────────────────────────────────────

    #[test]
    fn test_list_bucket_result_empty() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResult::from_list_result(
            "test-bucket",
            "",
            "",
            1000,
            "",
            &result,
        ));
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
                custom_metadata: Default::default(),
            }],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResult::from_list_result(
            "data", "", "", 1000, "", &result,
        ));
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
        let xml = to_s3_xml(&s3_xml::ListBucketResult::from_list_result(
            "mybucket", "", "", 1000, "/", &result,
        ));
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
        let xml = to_s3_xml(&s3_xml::ListBucketResult::from_list_result(
            "bucket", "", "", 10, "", &result,
        ));
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

    // ── parse_range_header ────────────────────────────────────────────

    #[test]
    fn test_parse_range_header_full_range() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=0-99".parse().unwrap());
        assert_eq!(parse_range_header(&headers), Some((0, Some(99))));
    }

    #[test]
    fn test_parse_range_header_open_end() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=100-".parse().unwrap());
        assert_eq!(parse_range_header(&headers), Some((100, None)));
    }

    #[test]
    fn test_parse_range_header_absent() {
        let headers = HeaderMap::new();
        assert_eq!(parse_range_header(&headers), None);
    }

    #[test]
    fn test_parse_range_header_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "pages=1-5".parse().unwrap());
        assert_eq!(parse_range_header(&headers), None);
    }

    #[test]
    fn test_parse_range_header_non_numeric() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=abc-def".parse().unwrap());
        assert_eq!(parse_range_header(&headers), None);
    }

    // ── ListBucketResultV2 ─────────────────────────────────────────────

    #[test]
    fn test_list_v2_has_key_count() {
        let result = ListResult {
            contents: vec![ObjectMetadata {
                key: "a.txt".to_string(),
                size: 10,
                last_modified: "2026-01-01T00:00:00Z".to_string(),
                etag: "\"abc\"".to_string(),
                content_type: "text/plain".to_string(),
                custom_metadata: Default::default(),
            }],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResultV2::from_list_result(
            "b", "", "", None, 1000, "", &result,
        ));
        assert!(xml.contains("<KeyCount>1</KeyCount>"));
        assert!(!xml.contains("<Marker>"));
    }

    #[test]
    fn test_list_v2_continuation_token_roundtrip() {
        use base64::Engine;
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: true,
            next_marker: Some("last-key".to_string()),
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResultV2::from_list_result(
            "b",
            "",
            "",
            Some("input-token"),
            10,
            "",
            &result,
        ));
        assert!(xml.contains("<ContinuationToken>input-token</ContinuationToken>"));
        assert!(xml.contains("<IsTruncated>true</IsTruncated>"));
        // NextContinuationToken should be base64 of "last-key"
        let expected_token = base64::engine::general_purpose::STANDARD.encode("last-key");
        assert!(xml.contains(&format!(
            "<NextContinuationToken>{}</NextContinuationToken>",
            expected_token
        )));
    }

    #[test]
    fn test_list_v2_start_after() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: None,
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResultV2::from_list_result(
            "b",
            "",
            "start-key",
            None,
            1000,
            "",
            &result,
        ));
        assert!(xml.contains("<StartAfter>start-key</StartAfter>"));
    }

    #[test]
    fn test_list_v2_no_next_token_when_not_truncated() {
        let result = ListResult {
            contents: vec![],
            common_prefixes: vec![],
            is_truncated: false,
            next_marker: Some("should-not-appear".to_string()),
        };
        let xml = to_s3_xml(&s3_xml::ListBucketResultV2::from_list_result(
            "b", "", "", None, 1000, "", &result,
        ));
        assert!(!xml.contains("NextContinuationToken"));
    }

    // ── CopyObjectResult ────────────────────────────────────────────────

    #[test]
    fn test_copy_result_has_etag_and_last_modified() {
        let xml = to_s3_xml(&s3_xml::CopyObjectResult {
            etag: "\"abc123\"".to_string(),
            last_modified: "2026-03-18T12:00:00Z".to_string(),
        });
        assert!(xml.contains("<CopyObjectResult>"));
        assert!(
            xml.contains("<ETag>\"abc123\"</ETag>")
                || xml.contains("<ETag>&quot;abc123&quot;</ETag>")
        );
        assert!(xml.contains("<LastModified>2026-03-18T12:00:00Z</LastModified>"));
    }

    // ── CompleteMultipartUpload deserialization ──────────────────────────

    #[test]
    fn test_parse_complete_multipart_request() {
        let xml = r#"<CompleteMultipartUpload>
            <Part><PartNumber>1</PartNumber><ETag>"aaa"</ETag></Part>
            <Part><PartNumber>2</PartNumber><ETag>"bbb"</ETag></Part>
        </CompleteMultipartUpload>"#;
        let req = s3_xml::from_s3_xml::<s3_xml::CompleteMultipartUploadRequest>(xml).unwrap();
        assert_eq!(req.parts.len(), 2);
        assert_eq!(req.parts[0].part_number, 1);
        assert_eq!(req.parts[1].part_number, 2);
    }

    // ── copy_source parsing (via validate helpers) ────────────────────

    #[test]
    fn test_copy_source_with_leading_slash_parses() {
        let source = "/src-bucket/path/to/key.txt";
        let stripped = source.trim_start_matches('/');
        let (bucket, key) = stripped.split_once('/').unwrap();
        assert_eq!(bucket, "src-bucket");
        assert_eq!(key, "path/to/key.txt");
    }

    #[test]
    fn test_copy_source_without_leading_slash_parses() {
        let source = "src-bucket/key.txt";
        let stripped = source.trim_start_matches('/');
        let (bucket, key) = stripped.split_once('/').unwrap();
        assert_eq!(bucket, "src-bucket");
        assert_eq!(key, "key.txt");
    }

    #[test]
    fn test_copy_source_no_key_fails() {
        let source = "bucket-only";
        let stripped = source.trim_start_matches('/');
        assert!(stripped.split_once('/').is_none());
    }

    // ── etag_matches ──────────────────────────────────────────────────

    #[test]
    fn test_etag_matches_exact() {
        assert!(etag_matches("\"abc123\"", "\"abc123\""));
    }

    #[test]
    fn test_etag_matches_without_quotes() {
        assert!(etag_matches("abc123", "\"abc123\""));
        assert!(etag_matches("\"abc123\"", "abc123"));
    }

    #[test]
    fn test_etag_matches_star() {
        assert!(etag_matches("*", "\"anything\""));
    }

    #[test]
    fn test_etag_matches_comma_separated() {
        assert!(etag_matches("\"aaa\", \"bbb\", \"ccc\"", "\"bbb\""));
    }

    #[test]
    fn test_etag_no_match() {
        assert!(!etag_matches("\"aaa\"", "\"bbb\""));
    }

    // ── evaluate_conditionals ─────────────────────────────────────────

    #[test]
    fn test_conditionals_proceed_when_no_headers() {
        let headers = HeaderMap::new();
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::Proceed
        ));
    }

    #[test]
    fn test_if_none_match_returns_not_modified() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, "\"etag1\"".parse().unwrap());
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::NotModified
        ));
    }

    #[test]
    fn test_if_none_match_different_etag_proceeds() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, "\"other\"".parse().unwrap());
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::Proceed
        ));
    }

    #[test]
    fn test_if_match_returns_precondition_failed() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, "\"wrong\"".parse().unwrap());
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::PreconditionFailed
        ));
    }

    #[test]
    fn test_if_match_correct_etag_proceeds() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, "\"etag1\"".parse().unwrap());
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::Proceed
        ));
    }

    #[test]
    fn test_if_match_star_proceeds() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, "*".parse().unwrap());
        assert!(matches!(
            evaluate_conditionals(&headers, "\"etag1\"", "2026-03-18T12:00:00Z"),
            ConditionalResult::Proceed
        ));
    }

    // ── extract_custom_metadata ───────────────────────────────────────

    #[test]
    fn test_extract_custom_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-meta-author", "alice".parse().unwrap());
        headers.insert("x-amz-meta-tag", "photo".parse().unwrap());
        headers.insert("content-type", "image/jpeg".parse().unwrap()); // not x-amz-meta-*

        let meta = extract_custom_metadata(&headers);
        assert_eq!(meta.len(), 2);
        assert_eq!(meta.get("author").unwrap(), "alice");
        assert_eq!(meta.get("tag").unwrap(), "photo");
    }

    #[test]
    fn test_extract_custom_metadata_empty() {
        let headers = HeaderMap::new();
        let meta = extract_custom_metadata(&headers);
        assert!(meta.is_empty());
    }
}
