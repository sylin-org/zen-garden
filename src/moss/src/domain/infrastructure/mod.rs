//! Infrastructure Handlers - Self-contained handlers for garden-wide effects
//!
//! When certain offerings are deployed anywhere in the garden, all Stones should
//! react by configuring their local infrastructure. This module provides the
//! framework for such handlers.
//!
//! ## Design Principles
//!
//! - **Self-contained**: Each handler knows what offerings it matches AND what actions to take
//! - **Local-only**: Handlers only affect the local Stone's infrastructure (no remote control)
//! - **Reactive**: Handlers are triggered after topology changes (chirps received)
//! - **SoC compliant**: Behavioral logic lives here, not in offering manifests
//!
//! ## Example: Docker Registry Handler
//!
//! When a container registry is planted anywhere in the garden, all Stones should
//! configure their local Docker daemon to trust that registry as an insecure source.
//! The handler:
//! 1. Matches offerings by the "container-registry" tag (from frontmatter.json)
//! 2. Collects all matching registries across the garden
//! 3. Updates local daemon.json with insecure-registries
//! 4. Restarts Docker daemon if the list changed
//!
//! ## Usage
//!
//! ```ignore
//! // Create handler registry during bootstrap
//! let handlers = InfrastructureHandlerRegistry::new(vec![
//!     Box::new(DockerRegistry::new()),
//! ]);
//!
//! // After topology update in coordinator
//! handlers.on_topology_changed(&topology_cache, &manifest_registry).await;
//! ```

mod docker_registry;

pub use docker_registry::DockerRegistry;

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::domain::topology::TopologyCache;
use garden_common::manifests::ManifestRegistry;

/// An instance of a matching offering discovered in the garden topology
#[derive(Debug, Clone)]
pub struct OfferingInstance {
    /// Stone name where the offering is deployed
    pub stone_name: String,
    /// Stone's API endpoint (e.g., "http://192.168.1.100:7185")
    pub stone_endpoint: String,
    /// Offering name (e.g., "registry", "zot")
    pub offering: String,
    /// Offering category (e.g., "devops")
    pub category: String,
    /// Offering tags from frontmatter
    pub tags: Vec<String>,
    /// Primary port from frontmatter (e.g., 5000 for registry)
    pub port: Option<u16>,
}

impl OfferingInstance {
    /// Extract the host portion from the stone endpoint
    /// e.g., "http://192.168.1.100:7185" -> "192.168.1.100"
    pub fn host(&self) -> Option<&str> {
        self.stone_endpoint
            .strip_prefix("http://")
            .or_else(|| self.stone_endpoint.strip_prefix("https://"))
            .and_then(|s| s.split(':').next())
    }
}

/// Trait for infrastructure handlers
///
/// Handlers are self-contained modules that:
/// 1. Know what offerings they care about (via `matches`)
/// 2. Know what local infrastructure to configure (via `sync`)
///
/// Handlers only affect LOCAL infrastructure - they never contact other Stones.
pub trait InfrastructureHandler: Send + Sync {
    /// Handler identifier for logging and debugging
    fn name(&self) -> &'static str;

    /// Check if this handler cares about the given offering
    ///
    /// Called for each offering in the garden topology.
    /// Return true to include this offering in the `sync` call.
    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool;

    /// Sync local infrastructure with current garden state
    ///
    /// Called with ALL matching offerings currently in the garden (not just changes).
    /// The handler should:
    /// 1. Compute desired state from instances
    /// 2. Compare with current local state
    /// 3. Apply changes only if needed
    ///
    /// If `instances` is empty, all matching offerings have been removed - clean up.
    fn sync<'a>(
        &'a self,
        instances: &'a [OfferingInstance],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

/// Registry of all infrastructure handlers
///
/// Manages a collection of handlers and dispatches topology changes to them.
pub struct InfrastructureHandlerRegistry {
    handlers: Vec<Box<dyn InfrastructureHandler>>,
}

impl InfrastructureHandlerRegistry {
    /// Create a new registry with the given handlers
    pub fn new(handlers: Vec<Box<dyn InfrastructureHandler>>) -> Self {
        let names: Vec<_> = handlers.iter().map(|h| h.name()).collect();
        tracing::info!(handlers = ?names, "Infrastructure handler registry initialized");
        Self { handlers }
    }

    /// Create an empty registry (for testing or when handlers are disabled)
    pub fn empty() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Called after topology changes to sync all handlers
    ///
    /// For each handler:
    /// 1. Scan topology for matching offerings
    /// 2. Build OfferingInstance list
    /// 3. Call handler's sync method
    ///
    /// Errors are logged but don't stop other handlers from running.
    pub async fn on_topology_changed(
        &self,
        topology_cache: &TopologyCache,
        manifest_registry: &Arc<ManifestRegistry>,
    ) {
        if self.handlers.is_empty() {
            return;
        }

        // Get all online stones from topology
        let stones = crate::domain::topology::get_online_stones(topology_cache).await;

        tracing::debug!(
            stone_count = stones.len(),
            total_services = stones.iter().map(|s| s.services.len()).sum::<usize>(),
            "Infrastructure handlers: processing topology"
        );

        for handler in &self.handlers {
            // Collect matching offerings across all stones
            let mut instances = Vec::new();

            for stone in &stones {
                for service in &stone.services {
                    // Get metadata from manifest registry
                    let manifest_entry = manifest_registry.sw.get(&service.offering);
                    let tags = manifest_entry.map(|entry| entry.tags()).unwrap_or_default();
                    let port = manifest_entry.and_then(|entry| entry.metadata.port);

                    if handler.matches(&service.offering, &service.category, &tags) {
                        instances.push(OfferingInstance {
                            stone_name: stone.stone_name.clone(),
                            stone_endpoint: stone.address.http_base(),
                            offering: service.offering.clone(),
                            category: service.category.clone(),
                            tags,
                            port,
                        });
                    }
                }
            }

            // Call handler with all matching instances
            let instance_count = instances.len();

            if instance_count > 0 {
                tracing::info!(
                    handler = handler.name(),
                    instances = instance_count,
                    offerings = ?instances.iter().map(|i| format!("{} on {}", i.offering, i.stone_name)).collect::<Vec<_>>(),
                    "Infrastructure handler: found matching instances"
                );
            }

            match handler.sync(&instances).await {
                Ok(()) => {
                    if instance_count > 0 {
                        tracing::debug!(
                            handler = handler.name(),
                            instances = instance_count,
                            "Infrastructure handler sync completed"
                        );
                    }
                }
                Err(e) => {
                    // Log with full error chain for debugging
                    tracing::warn!(
                        handler = handler.name(),
                        error = %e,
                        error_chain = ?e,
                        instances = instance_count,
                        "Infrastructure handler sync failed"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestHandler {
        name: &'static str,
        match_offering: String,
        sync_count: AtomicUsize,
    }

    impl TestHandler {
        fn new(name: &'static str, match_offering: &str) -> Self {
            Self {
                name,
                match_offering: match_offering.to_string(),
                sync_count: AtomicUsize::new(0),
            }
        }
    }

    impl InfrastructureHandler for TestHandler {
        fn name(&self) -> &'static str {
            self.name
        }

        fn matches(&self, offering: &str, _category: &str, _tags: &[String]) -> bool {
            offering == self.match_offering
        }

        fn sync<'a>(
            &'a self,
            _instances: &'a [OfferingInstance],
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                self.sync_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[test]
    fn test_offering_instance_host_extraction() {
        let instance = OfferingInstance {
            stone_name: "stone-01".to_string(),
            stone_endpoint: "http://192.168.1.100:7185".to_string(),
            offering: "registry".to_string(),
            category: "devops".to_string(),
            tags: vec![],
            port: Some(5000),
        };

        assert_eq!(instance.host(), Some("192.168.1.100"));

        let https_instance = OfferingInstance {
            stone_endpoint: "https://10.0.0.1:7185".to_string(),
            ..instance.clone()
        };
        assert_eq!(https_instance.host(), Some("10.0.0.1"));

        let hostname_instance = OfferingInstance {
            stone_endpoint: "http://stone-01.local:7185".to_string(),
            ..instance
        };
        assert_eq!(hostname_instance.host(), Some("stone-01.local"));
    }

    #[test]
    fn test_registry_creation() {
        let registry = InfrastructureHandlerRegistry::new(vec![
            Box::new(TestHandler::new("test-1", "registry")),
            Box::new(TestHandler::new("test-2", "zot")),
        ]);

        assert_eq!(registry.handlers.len(), 2);
    }

    #[test]
    fn test_empty_registry() {
        let registry = InfrastructureHandlerRegistry::empty();
        assert!(registry.handlers.is_empty());
    }
}
