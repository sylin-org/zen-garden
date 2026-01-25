//! Unified Manifest Registry - re-exported from garden_common
//!
//! This module now lives in `garden_common::manifests`.
//! All manifest loading logic is reusable.

// Re-export everything from common
pub use garden_common::manifests::{
    HwEntry, HwFrontmatter, HwManifests, RUNTIME_HW_MANIFESTS_DIR,
    SwEntry, SwFrontmatter, SwManifests, ServiceTemplate, TemplateInfo, RUNTIME_TEMPLATES_DIR,
    ManifestRegistry, RUNTIME_MANIFESTS_DIR,
};
