//! Image inspection — extract OCI metadata from Client images.
//!
//! Provides image-direct deployment with the port, volume, environment,
//! and label information needed to synthesize a deployment spec without
//! a curated manifest.

use crate::docker::Client;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashMap;

/// Result of inspecting a Client image's OCI config.
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
    /// OCI / Client labels
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

/// Pull an image (if not present) and inspect its OCI config.
///
/// Returns structured metadata extracted from the image's config layer.
pub async fn inspect_image(docker: &Client, image_ref: &str) -> Result<ImageInspection> {
    // Pull image first (no-op if already present, updates if tag changed)
    docker
        .pull_image(image_ref, None)
        .await
        .context("Failed to pull image for inspection")?;

    // Inspect the pulled image
    let inspect = docker
        .inspect_image_metadata(image_ref)
        .await
        .with_context(|| format!("Failed to inspect image '{}'", image_ref))?;

    let config = inspect
        .config
        .as_ref()
        .context("Image has no config layer")?;

    // Extract exposed ports: keys like "80/tcp", "443/tcp"
    let exposed_ports = config
        .exposed_ports
        .as_ref()
        .map(|ports| {
            ports
                .keys()
                .filter_map(|key| {
                    key.split('/')
                        .next()
                        .and_then(|port_str| port_str.parse::<u16>().ok())
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract volume mount points
    let volumes = config
        .volumes
        .as_ref()
        .map(|vols| vols.keys().cloned().collect())
        .unwrap_or_default();

    // Extract environment variables
    let environment = config.env.clone().unwrap_or_default();

    // Extract CMD
    let command = config.cmd.clone();

    // Extract ENTRYPOINT
    let entrypoint = config.entrypoint.clone();

    // Extract labels
    let labels = config.labels.clone().unwrap_or_default();

    // Extract healthcheck
    let healthcheck = config.healthcheck.as_ref().map(|hc| ImageHealthcheck {
        test: hc.test.clone().unwrap_or_default(),
        interval_ns: hc.interval,
        timeout_ns: hc.timeout,
        retries: hc.retries,
    });

    // Architecture from top-level inspect
    let architecture = inspect.architecture.clone();

    Ok(ImageInspection {
        image_ref: image_ref.to_string(),
        exposed_ports,
        volumes,
        environment,
        command,
        entrypoint,
        labels,
        healthcheck,
        architecture,
    })
}

/// Extract a human-readable description from OCI labels.
pub fn description_from_labels(labels: &HashMap<String, String>) -> Option<String> {
    // Try standard OCI annotation first, then Client-specific
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exposed_port_format() {
        // Simulate the key format Client uses: "80/tcp"
        let key = "8080/tcp";
        let port: u16 = key.split('/').next().unwrap().parse().unwrap();
        assert_eq!(port, 8080);
    }

    #[test]
    fn description_extraction() {
        let mut labels = HashMap::new();
        labels.insert(
            "org.opencontainers.image.description".to_string(),
            "A web server".to_string(),
        );
        assert_eq!(
            description_from_labels(&labels),
            Some("A web server".to_string())
        );

        let empty: HashMap<String, String> = HashMap::new();
        assert_eq!(description_from_labels(&empty), None);
    }
}
