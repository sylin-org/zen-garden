//! Manifest schemas for Zen Garden offerings
//!
//! Provides structured schemas for offering manifests that support
//! multiple deployment modes (managed, adopted, borrowed).

pub mod category;
pub mod ceremony;
pub mod detection;
pub mod hw;
pub mod offering;
pub mod ports;
pub mod registry;

pub use category::{
    CategoryConfig,
    CategoryRegistry,
    get_category_registry,
    init_category_registry,
    load_categories,
};

// Detection and control types
pub use detection::{
    OsDetectionRules,
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
pub use ports::{get_ports_catalog, init_ports_catalog, init_ports_catalog_from_str, load_ports_catalog};
pub use registry::{ManifestRegistry, discover_subdirectories, RUNTIME_MANIFESTS_DIR};

// Unified Offering Model
pub use offering::{
    Offering, OfferingRegistry, OfferingMetadata,
    ManagedConfig, AdoptedConfig, BorrowedConfig,
    ServiceTemplate, TemplateInfo, runtime_manifests_dir,
    NetworkRequirements, StaticIpPreference,
};
