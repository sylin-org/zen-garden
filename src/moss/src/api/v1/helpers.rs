//! Helper endpoints for internal operations
//!
//! These endpoints support capability discovery by providing JSON transformation
//! without requiring external tools like jq.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::AppState;

/// Request for JSON transformation
#[derive(Debug, Deserialize)]
pub struct JsonTransformRequest {
    /// Raw JSON input to transform
    pub input: serde_json::Value,

    /// Transformation specification
    pub transform: TransformSpec,
}

/// Transformation specification
#[derive(Debug, Deserialize)]
pub struct TransformSpec {
    /// JSONPath to the items array (e.g., ".models")
    pub items_path: String,

    /// Field mappings from source to CapabilityItem fields
    pub fields: FieldMappings,
}

/// Field mappings for transformation
#[derive(Debug, Deserialize)]
pub struct FieldMappings {
    /// Path to name field (required)
    pub name: String,

    /// Path to version field (optional)
    #[serde(default)]
    pub version: Option<String>,

    /// Path to size_bytes field (optional)
    #[serde(default)]
    pub size_bytes: Option<String>,

    /// Metadata field mappings (optional)
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Response from JSON transformation
#[derive(Debug, Serialize)]
pub struct JsonTransformResponse {
    /// Transformed items
    pub items: Vec<garden_common::CapabilityItem>,

    /// Number of items extracted
    pub count: usize,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct TransformError {
    pub error: String,
    pub details: Option<String>,
}

/// POST /api/v1/helpers/json-transform
///
/// Transform JSON input into normalized CapabilityItem format.
/// Used by capability discovery commands to avoid jq dependency.
pub async fn json_transform(
    State(_state): State<AppState>,
    Json(request): Json<JsonTransformRequest>,
) -> Result<Json<JsonTransformResponse>, (StatusCode, Json<TransformError>)> {
    // Extract items array using items_path
    let items_array = extract_path(&request.input, &request.transform.items_path)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(TransformError {
                    error: "Invalid items_path".to_string(),
                    details: Some(e),
                }),
            )
        })?;

    let array = items_array.as_array().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(TransformError {
                error: "items_path did not resolve to an array".to_string(),
                details: None,
            }),
        )
    })?;

    // Transform each item
    let mut items = Vec::with_capacity(array.len());
    for item in array {
        match transform_item(item, &request.transform.fields) {
            Ok(cap_item) => items.push(cap_item),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to transform item, skipping");
                continue;
            }
        }
    }

    let count = items.len();
    Ok(Json(JsonTransformResponse { items, count }))
}

/// Extract a value from JSON using a simple path notation
///
/// Supports paths like:
/// - ".field" - direct field access
/// - ".field.nested" - nested field access
/// - "." - root object
fn extract_path(value: &serde_json::Value, path: &str) -> Result<serde_json::Value, String> {
    let path = path.trim();

    // Handle root
    if path == "." {
        return Ok(value.clone());
    }

    // Remove leading dot if present
    let path = path.strip_prefix('.').unwrap_or(path);

    // Split by dots and navigate
    let mut current = value;
    for segment in path.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }

        current = current.get(segment).ok_or_else(|| {
            format!("Field '{}' not found in path '{}'", segment, path)
        })?;
    }

    Ok(current.clone())
}

/// Transform a single JSON object into a CapabilityItem
fn transform_item(
    item: &serde_json::Value,
    fields: &FieldMappings,
) -> Result<garden_common::CapabilityItem, String> {
    // Extract required name field
    let name = extract_path(item, &fields.name)?
        .as_str()
        .ok_or("name field is not a string")?
        .to_string();

    // Extract optional version
    let version = fields
        .version
        .as_ref()
        .and_then(|path| extract_path(item, path).ok())
        .and_then(|v| v.as_str().map(String::from));

    // Extract optional size_bytes
    let size_bytes = fields
        .size_bytes
        .as_ref()
        .and_then(|path| extract_path(item, path).ok())
        .and_then(|v| v.as_u64());

    // Compute human-readable size from bytes
    let size = size_bytes.map(format_bytes);

    // Extract metadata fields
    let mut metadata = HashMap::new();
    for (key, path) in &fields.metadata {
        if let Ok(value) = extract_path(item, path) {
            // Only include non-null values
            if !value.is_null() {
                metadata.insert(key.clone(), value);
            }
        }
    }

    Ok(garden_common::CapabilityItem {
        name,
        version,
        size,
        size_bytes,
        status: None,
        metadata,
    })
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_path_simple() {
        let value = json!({"name": "llama2", "size": 123});
        assert_eq!(
            extract_path(&value, ".name").unwrap(),
            json!("llama2")
        );
        assert_eq!(
            extract_path(&value, ".size").unwrap(),
            json!(123)
        );
    }

    #[test]
    fn test_extract_path_nested() {
        let value = json!({
            "details": {
                "family": "llama",
                "quantization": "Q4_0"
            }
        });
        assert_eq!(
            extract_path(&value, ".details.family").unwrap(),
            json!("llama")
        );
    }

    #[test]
    fn test_extract_path_root() {
        let value = json!({"a": 1});
        assert_eq!(extract_path(&value, ".").unwrap(), value);
    }

    #[test]
    fn test_transform_item() {
        let item = json!({
            "name": "llama2:7b",
            "size": 3826793472_u64,
            "details": {
                "family": "llama",
                "quantization": "Q4_0"
            }
        });

        let fields = FieldMappings {
            name: ".name".to_string(),
            version: None,
            size_bytes: Some(".size".to_string()),
            metadata: [
                ("family".to_string(), ".details.family".to_string()),
                ("quantization".to_string(), ".details.quantization".to_string()),
            ]
            .into_iter()
            .collect(),
        };

        let result = transform_item(&item, &fields).unwrap();
        assert_eq!(result.name, "llama2:7b");
        assert_eq!(result.size_bytes, Some(3826793472));
        assert_eq!(result.size, Some("3.6 GB".to_string()));
        assert_eq!(
            result.metadata.get("family"),
            Some(&json!("llama"))
        );
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
        assert_eq!(format_bytes(1_500_000), "1.4 MB");
        assert_eq!(format_bytes(3_826_793_472), "3.6 GB");
        assert_eq!(format_bytes(2_000_000_000_000), "1.8 TB");
    }
}
