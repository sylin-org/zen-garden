//! Unified Manifest Registry - re-exported from garden_common
//!
//! This module now lives in `garden_common::manifests`.
//! All manifest loading logic is reusable.

use std::collections::HashMap;
use std::sync::OnceLock;

// Re-export the unified offering types from common
pub use garden_common::manifests::{
    AdoptedConfig,
    BorrowedConfig,
    // Capability manifests
    CapabilityManifest,
    // Hardware manifests
    HwEntry,
    HwFrontmatter,
    HwManifests,
    ManagedConfig,
    ManifestRegistry,
    // Unified offering model
    Offering,
    OfferingMetadata,
    OfferingRegistry,
    RUNTIME_HW_MANIFESTS_DIR,
    RUNTIME_MANIFESTS_DIR,
    ServiceTemplate,
    TemplateInfo,
    runtime_manifests_dir,
};

use crate::infra::EmbeddedManifests;

// ============================================================================
// Capability Manifest Registry
// ============================================================================

/// Global capability manifest registry (lazy-loaded on first access)
static CAPABILITY_REGISTRY: OnceLock<HashMap<String, CapabilityManifest>> = OnceLock::new();

/// Load capability manifests from embedded resources
///
/// Returns a map of offering name -> CapabilityManifest
pub fn load_capability_manifests() -> &'static HashMap<String, CapabilityManifest> {
    CAPABILITY_REGISTRY.get_or_init(|| {
        let mut registry = HashMap::new();

        // Scan embedded manifests for *.capabilities.yaml files
        for file_path in EmbeddedManifests::list_files() {
            if file_path.ends_with(".capabilities.yaml")
                && let Some(content) = EmbeddedManifests::get_string(&file_path)
            {
                match CapabilityManifest::from_yaml(&content) {
                    Ok(manifest) => {
                        tracing::debug!(
                            offering = %manifest.offering,
                            path = %file_path,
                            "Loaded capability manifest"
                        );
                        registry.insert(manifest.offering.clone(), manifest);
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %file_path,
                            error = ?e,
                            "Failed to parse capability manifest"
                        );
                    }
                }
            }
        }

        tracing::info!(count = registry.len(), "Loaded capability manifests");

        registry
    })
}

/// Get capability manifest for a specific offering
pub fn get_capability_manifest(offering: &str) -> Option<&'static CapabilityManifest> {
    load_capability_manifests().get(offering)
}
