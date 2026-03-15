//! Image inspection value types and pure helper functions.
//!
//! These are domain-level data types extracted from infra so that
//! domain code (offering_resolution) can depend on them without
//! importing the I/O layer.

use serde::Serialize;
use std::collections::HashMap;

/// Result of inspecting a Docker image's OCI config.
#[derive(Debug, Clone, Serialize)]
pub struct ImageInspection {
    /// Original image reference (e.g., "nginx:latest")
    pub image_ref: String,
    /// Exposed ports extracted from image config
    pub exposed_ports: Vec<u16>,
    /// Volume mount points defined in image
    pub volumes: Vec<String>,
    /// Default environment variables from image
    pub environment: Vec<String>,
    /// Default CMD from image
    pub command: Option<Vec<String>>,
    /// Default ENTRYPOINT from image
    pub entrypoint: Option<Vec<String>>,
    /// OCI / Docker labels
    pub labels: HashMap<String, String>,
    /// Embedded HEALTHCHECK (if any)
    pub healthcheck: Option<ImageHealthcheck>,
    /// Image architecture (e.g., "amd64")
    pub architecture: Option<String>,
}

/// Embedded healthcheck from the image config.
#[derive(Debug, Clone, Serialize)]
pub struct ImageHealthcheck {
    pub test: Vec<String>,
    pub interval_ns: Option<i64>,
    pub timeout_ns: Option<i64>,
    pub retries: Option<i64>,
}

/// Extract a human-readable description from OCI labels.
pub fn description_from_labels(labels: &HashMap<String, String>) -> Option<String> {
    labels
        .get("org.opencontainers.image.description")
        .or_else(|| labels.get("description"))
        .cloned()
}

/// Extract a display title from OCI labels.
pub fn title_from_labels(labels: &HashMap<String, String>) -> Option<String> {
    labels
        .get("org.opencontainers.image.title")
        .or_else(|| labels.get("maintainer"))
        .cloned()
}
