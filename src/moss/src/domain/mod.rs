//! Domain layer - Business logic
//!
//! This layer contains core business logic for moss:
//! - Service lifecycle management
//! - Registry operations
//! - Offering management
//! - Compatibility checking
//! - Container adoption
//! - Service reconciliation
//! - Service discovery
//! - Ceremony orchestration
//!
//! Domain layer is pure business logic - no I/O.
//! All I/O goes through infra layer.

pub mod service_manager;
pub mod registry;
pub mod compatibility;
pub mod constraints;
pub mod offerings;
pub mod health;
pub mod adoption;
pub mod reconciliation;
pub mod modes;
pub mod scoring;
pub mod metrics_collection;
pub mod topology;
pub mod services;
pub mod placement;
pub mod connection;
pub mod service_discovery;
pub mod ceremony;
pub mod harvest;
pub mod storage_cache;

pub use service_manager::ServiceManager;
pub use registry::Registry;
pub use compatibility::{
    CompatCheckCapabilities, CompatibilityDecision, CompiledCompatibility,
    get_current_compat_capabilities, compile_compatibility, evaluate_compatibility,
    validate_binary_architecture,
};
pub use offerings::{
    CompiledOffering, OfferingsFingerprint, OfferingsIndexCache,
    moss_version_string, current_capabilities_hash, manifests_hash, rebuild_offerings_index,
    ensure_offerings_index, get_compiled_offering,
};
pub use health::{
    check_disk_health, check_memory_health,
    build_disk_component, build_memory_component,
    determine_overall_status,
};
pub use adoption::{
    adopt_offering_container, adopt_existing_containers, AdoptionResult,
};
pub use reconciliation::{
    reconcile_services, ReconciliationResult,
};
pub use modes::{
    DetectionOrchestrator, AggregatedDetectionResult,
};
pub use connection::{
    ResolvedConnection, resolve_connection, extract_ip, build_hostname,
    default_template, infer_protocol, resolve_uris,
};
pub use service_discovery::{
    ServiceSearchCriteria, FoundService, StoneRef, ServiceDiscoveryResponse,
    find_services, find_local_services, list_all_local_services,
};
// Re-export TopologyEntry from common (now shared type)
pub use garden_common::TopologyEntry;
pub use ceremony::{
    execute_nourish_offering, Ceremony, CeremonyId, CeremonyInitiator, CeremonyOptions,
    CeremonyRegistry, CeremonyState, CeremonyType, Phase, PhaseState,
};
pub use harvest::{HarvestId, HarvestManifest, VolumeArchive};
pub use storage_cache::{
    StorageCache, StorageCacheInner, new_storage_cache,
    update_from_beacon, remove_stone as remove_stone_storage, find_s3_gateways, find_by_name,
};
// Categories are now data-driven via garden_common::manifests::get_category_registry()
