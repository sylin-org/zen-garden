//! Image inspection — extract OCI metadata from Docker images.
//!
//! Provides image-direct deployment with the port, volume, environment,
//! and label information needed to synthesize a deployment spec without
//! a curated manifest.
//!
//! Value types (`ImageInspection`, `ImageHealthcheck`) and pure label
//! helpers live in `domain::image_types`; this module provides the I/O
//! function that populates them.

use crate::docker::Client;
use anyhow::{Context, Result};

// Re-export domain value types so existing `use crate::infra::image_inspect::*`
// callers outside domain continue to compile.
pub use crate::domain::image_types::{
    description_from_labels, title_from_labels, ImageHealthcheck, ImageInspection,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parse_exposed_port_format() {
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
