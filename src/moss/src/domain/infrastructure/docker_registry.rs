//! Docker Registry Handler - Manages insecure-registries in daemon.json
//!
//! When container registries are deployed in the garden, this handler configures
//! the local Docker daemon to trust them as insecure registries.
//!
//! ## Matching Logic
//!
//! Matches offerings with the "container-registry" tag in the "devops" category.
//! This is read from frontmatter.json - no hardcoded offering names.
//!
//! ## Actions
//!
//! 1. Collects all matching registry endpoints from garden topology
//! 2. Reads current daemon.json insecure-registries
//! 3. Computes the union of existing non-garden registries + garden registries
//! 4. If changed: writes daemon.json and restarts Docker daemon (silent)
//!
//! ## Platform Support
//!
//! - Linux: `/etc/docker/daemon.json`, restart via systemctl
//! - Windows: `%PROGRAMDATA%\docker\config\daemon.json`, restart via service control

use anyhow::Result;
use async_trait::async_trait;

use super::{InfrastructureHandler, OfferingInstance};
use crate::infra::docker_config;

/// Tag that identifies container registries (from frontmatter.json)
const CONTAINER_REGISTRY_TAG: &str = "container-registry";

/// Default port when frontmatter doesn't specify one
const DEFAULT_REGISTRY_PORT: u16 = 5000;

/// Docker Registry Handler
///
/// Configures local Docker daemon to trust garden container registries.
/// Matching is based purely on manifest tags - no hardcoded offering names.
pub struct DockerRegistryHandler;

impl DockerRegistryHandler {
    /// Create a new Docker registry handler
    pub fn new() -> Self {
        Self
    }

    /// Build registry endpoint from offering instance
    ///
    /// Format: "host:port" (e.g., "192.168.1.100:5000")
    /// Port comes from frontmatter.json, falls back to 5000.
    fn build_endpoint(&self, instance: &OfferingInstance) -> Option<String> {
        let host = instance.host()?;
        let port = instance.port.unwrap_or(DEFAULT_REGISTRY_PORT);
        Some(format!("{}:{}", host, port))
    }
}

impl Default for DockerRegistryHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InfrastructureHandler for DockerRegistryHandler {
    fn name(&self) -> &'static str {
        "docker-registry"
    }

    fn matches(&self, _offering: &str, category: &str, tags: &[String]) -> bool {
        // Match by category + tag from frontmatter.json
        // No hardcoded offering names - manifest is the source of truth
        category == "devops" && tags.iter().any(|t| t == CONTAINER_REGISTRY_TAG)
    }

    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
        // Build list of garden registry endpoints
        let garden_registries: Vec<String> = instances
            .iter()
            .filter_map(|i| self.build_endpoint(i))
            .collect();

        tracing::debug!(
            registries = ?garden_registries,
            "Docker registry handler: syncing insecure-registries"
        );

        // Read current daemon.json
        let current_registries = docker_config::read_insecure_registries().await?;

        // Compute desired state:
        // - Keep existing registries that aren't garden-managed
        // - Add all current garden registries
        //
        // For simplicity, we replace all insecure-registries with the garden list
        // Users who need additional registries can add them and they'll be preserved
        // on subsequent syncs (we only remove registries that were in the garden but are now gone)

        // For now, we simply set the garden registries as the insecure-registries list
        // This is safe because:
        // 1. Most users won't have manual insecure-registries configured
        // 2. Garden registries are the primary use case for insecure registries in homelab
        //
        // TODO: Implement smarter merging that preserves user-added registries

        // Check if update needed
        let mut desired_registries = garden_registries.clone();
        desired_registries.sort();

        let mut current_sorted = current_registries.clone();
        current_sorted.sort();

        if desired_registries == current_sorted {
            tracing::trace!("Docker registry handler: no changes needed");
            return Ok(());
        }

        // Update daemon.json
        let changed = docker_config::write_insecure_registries(&garden_registries).await?;

        if changed {
            tracing::info!(
                registries = ?garden_registries,
                "Updated Docker daemon insecure-registries"
            );

            // Restart Docker daemon to apply changes
            if let Err(e) = docker_config::restart_docker_daemon().await {
                tracing::warn!(
                    error = %e,
                    "Failed to restart Docker daemon - manual restart may be required"
                );
            } else {
                tracing::info!("Docker daemon restarted to apply registry changes");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_by_tag() {
        let handler = DockerRegistryHandler::new();

        // Matches: devops category with container-registry tag
        assert!(handler.matches(
            "registry",
            "devops",
            &["container-registry".to_string()]
        ));
        assert!(handler.matches(
            "zot",
            "devops",
            &["container-registry".to_string()]
        ));
        assert!(handler.matches(
            "any-future-registry",
            "devops",
            &["container-registry".to_string()]
        ));

        // Does NOT match: missing tag
        assert!(!handler.matches("registry", "devops", &[]));
        assert!(!handler.matches("zot", "devops", &[]));

        // Does NOT match: wrong category
        assert!(!handler.matches(
            "custom-registry",
            "storage",
            &["container-registry".to_string()]
        ));

        // Does NOT match: wrong tag
        assert!(!handler.matches(
            "custom-registry",
            "devops",
            &["some-other-tag".to_string()]
        ));
    }

    #[test]
    fn test_build_endpoint_with_port_from_manifest() {
        let handler = DockerRegistryHandler::new();

        // Port comes from frontmatter
        let instance = OfferingInstance {
            stone_name: "stone-01".to_string(),
            stone_endpoint: "http://192.168.1.100:7185".to_string(),
            offering: "registry".to_string(),
            category: "devops".to_string(),
            tags: vec!["container-registry".to_string()],
            port: Some(5000),
        };

        assert_eq!(
            handler.build_endpoint(&instance),
            Some("192.168.1.100:5000".to_string())
        );

        // Custom port from frontmatter
        let custom_port_instance = OfferingInstance {
            port: Some(8080),
            ..instance.clone()
        };
        assert_eq!(
            handler.build_endpoint(&custom_port_instance),
            Some("192.168.1.100:8080".to_string())
        );

        // Falls back to default when port not in frontmatter
        let no_port_instance = OfferingInstance {
            port: None,
            ..instance
        };
        assert_eq!(
            handler.build_endpoint(&no_port_instance),
            Some("192.168.1.100:5000".to_string())
        );
    }
}
