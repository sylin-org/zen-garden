//! Manifest schemas for Zen Garden offerings
//!
//! Provides structured schemas for offering manifests that support
//! multiple deployment modes (managed, adopted, borrowed).

pub mod capabilities;
pub mod category;
pub mod ceremony;
pub mod connection;
pub mod connectivity;
pub mod detection;
pub mod generate;
pub mod hw;
pub mod offering;
pub mod ports;
pub mod registry;
pub mod validation;

pub use category::{
    CategoryConfig, CategoryRegistry, get_category_registry, init_category_registry,
    load_categories,
};

// Detection and control types
pub use detection::{
    CommandDetection, ContainerInspectDetection, ControlConfig, DetectionConfig, DetectionMethod,
    DetectionRule, HealthConfig, HealthVerification, HttpProbeDetection, LocationConfig,
    OsDetectionRules, PortDetectionConfig, ProcessDetection,
};

pub use ceremony::{CeremonyMode, CeremonyPolicy, ExecConfig, RollbackConfig};

pub use connectivity::{
    CommandAction as ConnectivityCommandAction, ConnectivityConfig, ConnectivityRules,
};

// Re-export manifest loaders
pub use hw::{HwEntry, HwFrontmatter, HwManifests, RUNTIME_HW_MANIFESTS_DIR};
pub use ports::{
    get_ports_catalog, init_ports_catalog, init_ports_catalog_from_str, load_ports_catalog,
};
pub use registry::{ManifestRegistry, RUNTIME_MANIFESTS_DIR, discover_subdirectories};

// Unified Offering Model
pub use offering::{
    AdoptedConfig, BorrowedConfig, ConfigFileMapping, GpuDeviceRequest, ManageableEnv,
    ManagedConfig, NetworkRequirements, Offering, OfferingMetadata, OfferingRegistry,
    ServiceTemplate, StaticIpPreference, TemplateInfo, runtime_manifests_dir,
};

pub use connection::ConnectionProfile;

// Capability Manifest Schema
pub use capabilities::{
    AddOperationConfig, CapabilityDisplayConfig, CapabilityManifest, CapabilityTypeConfig,
    FieldMappings, ListOperationConfig, ModeCommands, MutabilityMode, OutputFormat,
    PlatformCommands, ProgressConfig, RemoveOperationConfig, SummaryConfig, TransformSpec,
};
