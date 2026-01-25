//! Manifest schemas for Zen Garden offerings
//!
//! Provides structured schemas for offering manifests that support
//! multiple deployment modes (managed, adopted, borrowed).

pub mod category;
pub mod ceremony;
pub mod offering;
pub mod hw;
pub mod sw;
pub mod registry;

pub use category::{
    CategoryConfig,
    CategoryRegistry,
    get_category_registry,
    init_category_registry,
    load_categories,
};

pub use offering::{
    OfferingManifest,
    DetectionRule,
    DetectionMethod,
    DetectionConfig,
    CommandDetection,
    ContainerInspectDetection,
    HttpProbeDetection,
    ControlConfig,
    LocationConfig,
    HealthConfig,
};

pub use ceremony::{
    CeremonyMode,
    CeremonyPolicy,
    ExecConfig,
    RollbackConfig,
};

// Re-export manifest loaders
pub use hw::{HwEntry, HwFrontmatter, HwManifests, RUNTIME_HW_MANIFESTS_DIR};
pub use sw::{SwEntry, SwFrontmatter, SwManifests, ServiceTemplate, TemplateInfo, RUNTIME_TEMPLATES_DIR};
pub use registry::{ManifestRegistry, discover_subdirectories, RUNTIME_MANIFESTS_DIR};
