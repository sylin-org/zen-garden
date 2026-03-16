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

pub mod adoption;
pub mod capabilities;
pub mod ceremony;
pub mod cloud_drive;
pub mod companion;
pub mod current;
pub mod discovery;
pub mod orchestration;
pub mod config_compose;
pub mod compatibility;
pub mod connection;
pub mod connectivity;
pub mod constraints;
pub mod events;
pub mod fitness;
pub mod garden_registry;
pub mod harvest;
pub mod health;
pub mod image_types;
pub mod infrastructure;
pub mod maintenance;
pub mod metrics_collection;
pub mod modes;
pub mod naming;
pub mod network;
pub mod nurturing;
pub mod offering_lifecycle;
pub mod offering_resolution;
pub mod offerings;
pub mod placement;
pub mod pond;
pub mod reconciliation;
pub mod registry;
pub mod scoring;
pub mod storage;
pub mod storage_service;
pub mod service_discovery;
pub mod service_lifecycle;
pub mod service_manager;
pub mod services;
pub mod services_internal;
pub mod fqn_handler;
pub mod platform;
pub mod presence;
pub mod security;
pub mod tool;
pub mod task_registry;
pub mod tools;
pub mod topology;
pub mod traits;

pub use adoption::{adopt_existing_containers, adopt_offering_container, AdoptionResult};
pub use compatibility::{
    compile_compatibility, evaluate_compatibility, get_current_compat_capabilities,
    validate_binary_architecture, CompatCheckCapabilities, CompatibilityDecision,
    CompiledCompatibility,
};
pub use connection::{
    build_hostname, default_template, extract_ip, infer_protocol, resolve_connection, resolve_uris,
    ResolvedConnection,
};
pub use connectivity::{ConnectivityOrchestrator, ConnectivityOutcome, ConnectivityStatus};
pub use health::{
    build_disk_component, build_memory_component, check_disk_health, check_memory_health,
    determine_overall_status,
};
pub use modes::{AggregatedDetectionResult, DetectionOrchestrator};
pub use offerings::{
    current_capabilities_hash, ensure_offerings_index, get_compiled_offering, manifests_hash,
    moss_version_string, rebuild_offerings_index, CompiledOffering, OfferingsFingerprint,
    OfferingsIndex,
};
pub use reconciliation::{reconcile_services, ReconciliationResult};
pub use registry::Registry;
pub use orchestration::{
    NourishmentOrchestration, NurturingOrchestration, Orchestration, StorageOrchestration,
};
pub use storage::{
    new_media, new_volumes, Management, Media, Medium, Storage, StorageBank, Volume,
    VolumeState, Volumes,
};
pub use storage_service::{LocalStorage, ProxyTarget, StorageRoute};
pub use service_discovery::{
    find_local_services, find_services, get_offering_port, list_all_local_services, FoundService,
    ServiceDiscoveryResponse, ServiceSearchCriteria, StoneRef,
};
pub use service_manager::ServiceLifecycle;
pub use cloud_drive::{classify_rename, DriveAction};
// Re-export TopologyEntry from common (now shared type)
pub use capabilities::{CapabilityExecutor, CapabilityMutationResult, Executor};
pub use ceremony::{
    execute_nourish_offering, Ceremony, CeremonyId, CeremonyInitiator, CeremonyOptions,
    CeremonyRegistry, CeremonyState, CeremonyType, Phase, PhaseState,
};
pub use events::{DomainEvent, OfferingEvent, PondEvent, StoneEvent, StorageEvent};
pub use harvest::{HarvestId, HarvestManifest, VolumeArchive};
pub use infrastructure::{
    DockerRegistry, InfrastructureHandler, InfrastructureHandlerRegistry, OfferingInstance,
};
pub use network::{
    NetworkError, NetworkMode, PoolExhausted, ProbeResult, StaticIpActive, StaticIpDesired,
    StaticIpRelease, StaticIpRequest, StaticIpSeverity, StaticIpState,
};
pub use nurturing::{
    NurturingIndex, NurturingResult, NurturingSlot, NurturingSnapshot, OfferingSlots,
    snapshot_from_harvest,
};
pub use pond::{load_pond_metadata, save_pond_metadata, PondMetadata, PondState};
pub use garden_registry::{
    new_registry, EntryOrigin, GardenRegistry, GardenRegistryInner, RegistryEntry, ToolQuery,
};
pub use companion::Companion;
pub use current::{Current, Stone as CurrentStone, Topology as CurrentTopology};
pub use discovery::Discovery;
pub use platform::Platform;
pub use presence::Presence;
pub use fqn_handler::{FqnHandler, FqnHandlerEntry, FqnHandlerRegistry};
pub use security::{Pond, Security};
pub use tool::Tool;
pub use tools::{
    stream_event_type_for_delta, ToolsSnapshotPayload,
};
// Categories are now data-driven via garden_common::manifests::get_category_registry()
