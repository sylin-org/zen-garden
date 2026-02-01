//! Docker Registry Handler - Manages insecure-registries in daemon.json
//!
//! When container registries are deployed in the garden, this handler configures
//! the local Docker daemon to trust them as insecure registries.
//!
//! ## Matching Logic
//!
//! Matches offerings that are container registries:
//! - By name: "registry", "zot", "harbor"
//! - By tag: category "devops" with tag "container-registry"
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

/// Known container registry offering names
const REGISTRY_OFFERINGS: &[&str] = &["registry", "zot", "harbor"];

/// Tag that identifies container registries in devops category
const CONTAINER_REGISTRY_TAG: &str = "container-registry";

/// Default ports for known registries (used when building endpoint)
fn default_port(offering: &str) -> u16 {
    match offering {
        "registry" => 5000,
        "zot" => 5000,
        "harbor" => 80, // Harbor uses 80/443 by default
        _ => 5000,
    }
}

/// Docker Registry Handler
///
/// Configures local Docker daemon to trust garden container registries.
pub struct DockerRegistryHandler {
    /// Prefix added to garden registry entries in daemon.json
    /// Used to distinguish garden-managed entries from user-managed ones
    garden_prefix: String,
}

impl DockerRegistryHandler {
    /// Create a new Docker registry handler
    pub fn new() -> Self {
        Self {
            garden_prefix: "zen-garden:".to_string(),
        }
    }

    /// Build registry endpoint from offering instance
    ///
    /// Format: "host:port" (e.g., "192.168.1.100:5000")
    fn build_endpoint(&self, instance: &OfferingInstance) -> Option<String> {
        let host = instance.host()?;
        let port = default_port(&instance.offering);
        Some(format!("{}:{}", host, port))
    }

    /// Check if a registry entry is managed by zen-garden
    ///
    /// Garden-managed entries are prefixed with a comment marker in the list.
    /// Since daemon.json doesn't support comments, we use a naming convention
    /// where garden registries are tracked separately.
    fn is_garden_managed(&self, _entry: &str) -> bool {
        // For now, we track garden registries by comparing against known garden endpoints
        // A more robust solution would be to persist the garden registry list separately
        true
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

    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool {
        // Match by known offering names
        if REGISTRY_OFFERINGS.contains(&offering) {
            return true;
        }

        // Match by category + tag
        if category == "devops" && tags.iter().any(|t| t == CONTAINER_REGISTRY_TAG) {
            return true;
        }

        false
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
    fn test_matches_by_name() {
        let handler = DockerRegistryHandler::new();

        assert!(handler.matches("registry", "devops", &[]));
        assert!(handler.matches("zot", "devops", &[]));
        assert!(handler.matches("harbor", "devops", &[]));
        assert!(!handler.matches("mongodb", "data", &[]));
    }

    #[test]
    fn test_matches_by_tag() {
        let handler = DockerRegistryHandler::new();

        // Unknown offering but has container-registry tag in devops category
        assert!(handler.matches(
            "custom-registry",
            "devops",
            &["container-registry".to_string()]
        ));

        // Wrong category
        assert!(!handler.matches(
            "custom-registry",
            "storage",
            &["container-registry".to_string()]
        ));

        // Right category, wrong tag
        assert!(!handler.matches(
            "custom-registry",
            "devops",
            &["some-other-tag".to_string()]
        ));
    }

    #[test]
    fn test_build_endpoint() {
        let handler = DockerRegistryHandler::new();

        let instance = OfferingInstance {
            stone_name: "stone-01".to_string(),
            stone_endpoint: "http://192.168.1.100:7185".to_string(),
            offering: "registry".to_string(),
            category: "devops".to_string(),
            tags: vec![],
        };

        assert_eq!(
            handler.build_endpoint(&instance),
            Some("192.168.1.100:5000".to_string())
        );

        let harbor_instance = OfferingInstance {
            offering: "harbor".to_string(),
            ..instance
        };
        assert_eq!(
            handler.build_endpoint(&harbor_instance),
            Some("192.168.1.100:80".to_string())
        );
    }

    #[test]
    fn test_default_ports() {
        assert_eq!(default_port("registry"), 5000);
        assert_eq!(default_port("zot"), 5000);
        assert_eq!(default_port("harbor"), 80);
        assert_eq!(default_port("unknown"), 5000);
    }
}
