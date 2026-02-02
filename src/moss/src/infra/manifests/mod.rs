//! Unified Manifest Registry - re-exported from garden_common
//!
//! This module now lives in `garden_common::manifests`.
//! All manifest loading logic is reusable.

// Re-export the unified offering types from common
pub use garden_common::manifests::{
    // Hardware manifests
    HwEntry, HwFrontmatter, HwManifests, RUNTIME_HW_MANIFESTS_DIR,
    // Unified offering model
    Offering, OfferingRegistry, OfferingMetadata,
    ManagedConfig, AdoptedConfig, BorrowedConfig,
    ServiceTemplate, TemplateInfo, runtime_manifests_dir,
    ManifestRegistry, RUNTIME_MANIFESTS_DIR,
};
