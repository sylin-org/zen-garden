//! S3 XML response/request types for quick-xml serde serialization.
//!
//! Replaces hand-built XML string concatenation with typed structs.
//! All response types implement `Serialize`, request types implement `Deserialize`.

use serde::{Deserialize, Serialize};

// ============================================================================
// Constants
// ============================================================================

const S3_XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

// ============================================================================
// Response types (Serialize)
// ============================================================================

/// S3 Error response body
#[derive(Serialize)]
#[serde(rename = "Error")]
pub struct S3Error {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

/// ListAllMyBucketsResult (GET /)
#[derive(Serialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "Buckets")]
    pub buckets: Buckets,
}

impl ListAllMyBucketsResult {
    pub fn new(bucket_names: &[String]) -> Self {
        Self {
            xmlns: S3_XMLNS,
            owner: Owner {
                id: "zen-garden".to_string(),
                display_name: "zen-garden".to_string(),
            },
            buckets: Buckets {
                bucket: bucket_names
                    .iter()
                    .map(|name| BucketEntry {
                        name: name.clone(),
                        creation_date: "2025-01-01T00:00:00.000Z".to_string(),
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Serialize)]
pub struct Owner {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

#[derive(Serialize)]
pub struct Buckets {
    #[serde(rename = "Bucket", default)]
    pub bucket: Vec<BucketEntry>,
}

#[derive(Serialize)]
pub struct BucketEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
}

/// ListBucketResult V1 (GET /{bucket})
#[derive(Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "Marker")]
    pub marker: String,
    #[serde(rename = "MaxKeys")]
    pub max_keys: usize,
    #[serde(rename = "Delimiter", skip_serializing_if = "String::is_empty")]
    pub delimiter: String,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Contents", default)]
    pub contents: Vec<S3Object>,
    #[serde(rename = "CommonPrefixes", default)]
    pub common_prefixes: Vec<CommonPrefix>,
}

impl ListBucketResult {
    pub fn from_list_result(
        bucket: &str,
        prefix: &str,
        marker: &str,
        max_keys: usize,
        delimiter: &str,
        result: &crate::infra::storage::ListResult,
    ) -> Self {
        Self {
            xmlns: S3_XMLNS,
            name: bucket.to_string(),
            prefix: prefix.to_string(),
            marker: marker.to_string(),
            max_keys,
            delimiter: delimiter.to_string(),
            is_truncated: result.is_truncated,
            contents: result.contents.iter().map(S3Object::from_metadata).collect(),
            common_prefixes: result
                .common_prefixes
                .iter()
                .map(|p| CommonPrefix {
                    prefix: p.clone(),
                })
                .collect(),
        }
    }
}

/// ListBucketResult V2 (GET /{bucket}?list-type=2)
#[derive(Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResultV2 {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "KeyCount")]
    pub key_count: usize,
    #[serde(rename = "MaxKeys")]
    pub max_keys: usize,
    #[serde(rename = "StartAfter", skip_serializing_if = "String::is_empty")]
    pub start_after: String,
    #[serde(rename = "Delimiter", skip_serializing_if = "String::is_empty")]
    pub delimiter: String,
    #[serde(
        rename = "ContinuationToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub continuation_token: Option<String>,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(
        rename = "NextContinuationToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_continuation_token: Option<String>,
    #[serde(rename = "Contents", default)]
    pub contents: Vec<S3Object>,
    #[serde(rename = "CommonPrefixes", default)]
    pub common_prefixes: Vec<CommonPrefix>,
}

impl ListBucketResultV2 {
    pub fn from_list_result(
        bucket: &str,
        prefix: &str,
        start_after: &str,
        continuation_token: Option<&str>,
        max_keys: usize,
        delimiter: &str,
        result: &crate::infra::storage::ListResult,
    ) -> Self {
        use base64::Engine;

        let next_continuation_token = if result.is_truncated {
            result.next_marker.as_ref().map(|marker| {
                base64::engine::general_purpose::STANDARD.encode(marker)
            })
        } else {
            None
        };

        Self {
            xmlns: S3_XMLNS,
            name: bucket.to_string(),
            prefix: prefix.to_string(),
            key_count: result.contents.len(),
            max_keys,
            start_after: start_after.to_string(),
            delimiter: delimiter.to_string(),
            continuation_token: continuation_token.map(|s| s.to_string()),
            is_truncated: result.is_truncated,
            next_continuation_token,
            contents: result.contents.iter().map(S3Object::from_metadata).collect(),
            common_prefixes: result
                .common_prefixes
                .iter()
                .map(|p| CommonPrefix {
                    prefix: p.clone(),
                })
                .collect(),
        }
    }
}

/// Individual object entry within a list result
#[derive(Serialize)]
pub struct S3Object {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "StorageClass")]
    pub storage_class: &'static str,
}

impl S3Object {
    fn from_metadata(meta: &crate::infra::storage::ObjectMetadata) -> Self {
        Self {
            key: meta.key.clone(),
            last_modified: meta.last_modified.clone(),
            etag: meta.etag.clone(),
            size: meta.size,
            storage_class: "STANDARD",
        }
    }
}

/// Common prefix entry (for delimiter-based grouping)
#[derive(Serialize)]
pub struct CommonPrefix {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

/// CopyObjectResult response
#[derive(Serialize)]
#[serde(rename = "CopyObjectResult")]
pub struct CopyObjectResult {
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
}

/// InitiateMultipartUploadResult response
#[derive(Serialize)]
#[serde(rename = "InitiateMultipartUploadResult")]
pub struct InitiateMultipartUploadResult {
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "UploadId")]
    pub upload_id: String,
}

/// CompleteMultipartUploadResult response
#[derive(Serialize)]
#[serde(rename = "CompleteMultipartUploadResult")]
pub struct CompleteMultipartUploadResult {
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "ETag")]
    pub etag: String,
}

// ============================================================================
// Request types (Deserialize)
// ============================================================================

/// CompleteMultipartUpload request body (parsed from client XML)
#[derive(Deserialize)]
#[serde(rename = "CompleteMultipartUpload")]
pub struct CompleteMultipartUploadRequest {
    #[serde(rename = "Part")]
    pub parts: Vec<CompletePart>,
}

/// Individual part within a CompleteMultipartUpload request
#[derive(Deserialize)]
pub struct CompletePart {
    #[serde(rename = "PartNumber")]
    pub part_number: u16,
    #[serde(rename = "ETag")]
    pub etag: String,
}

// ============================================================================
// Serialization helpers
// ============================================================================

/// Serialize any S3 response type to XML string with declaration.
pub fn to_s3_xml<T: Serialize>(value: &T) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    match quick_xml::se::to_string(value) {
        Ok(body) => {
            xml.push_str(&body);
            xml
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize S3 XML response");
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>InternalError</Code><Message>XML serialization failed</Message></Error>"#
            )
        }
    }
}

/// Deserialize S3 XML request body.
pub fn from_s3_xml<T: for<'de> Deserialize<'de>>(xml: &str) -> Result<T, String> {
    quick_xml::de::from_str(xml).map_err(|e| e.to_string())
}
