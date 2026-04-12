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
pub mod announcement;
pub mod capabilities;
pub mod catalog;
pub mod ceremony;
pub mod cloud_drive;
pub mod companion;
pub mod compatibility;
pub mod config_compose;
pub mod connection;
pub mod connectivity;
pub mod constraints;
pub mod current;
pub mod discovery;
pub mod events;
pub mod fitness;
pub mod harvest;
pub mod health;
pub mod image_types;
pub mod infrastructure;
pub mod jobs;
pub mod maintenance;
pub mod metrics;
pub mod modes;
pub mod naming;
pub mod network;
pub mod nurturing;
pub mod offering_lifecycle;
pub mod offering_resolution;
pub mod offerings;
pub mod orchestration;
pub mod placement;
pub mod platform;
pub mod pond;
pub mod presence;
pub mod reconciliation;
pub mod registry;
pub mod resources_collection;
pub mod scoring;
pub mod security;
pub mod service_discovery;
pub mod service_lifecycle;
pub mod service_manager;
pub mod services;
pub mod services_internal;
pub mod storage;
pub mod storage_service;
pub mod subsystems;
pub mod task_registry;
pub mod tool;
pub mod topology;
pub mod traits;

pub use adoption::{AdoptionResult, adopt_existing_containers, adopt_offering_container};
pub use cloud_drive::{DriveAction, classify_rename};
// compatibility: use crate::domain::compatibility::{...} directly
pub use catalog::{
    Catalog, CatalogCache, CatalogChangeKind, CatalogChanged, CatalogError, CatalogStats,
    CompiledOffering, FileCatalogCache, LoadSource, OfferingsFingerprint, OfferingsIndex,
};
pub use connection::{
    ResolvedConnection, build_hostname, default_template, extract_ip, infer_protocol,
    resolve_connection, resolve_uris,
};
pub use connectivity::{ConnectivityOrchestrator, ConnectivityOutcome, ConnectivityStatus};
pub use health::{
    DockerHealthProbe, Health, HealthChangeKind, HealthChanged, HealthProbe, HealthProbeResult,
    build_disk_component, build_memory_component, check_disk_health, check_memory_health,
    determine_overall_status,
};
pub use metrics::{
    DomainSnapshot as MetricsDomainSnapshot, GlobalSnapshot as MetricsGlobalSnapshot,
    LatencySnapshot as MetricsLatencySnapshot, Metrics, MetricsChanged, MetricsError,
    MetricsSnapshot, TaskSnapshot as MetricsTaskSnapshot,
};
pub use modes::{AggregatedDetectionResult, DetectionOrchestrator};
pub use offerings::{
    ActiveGuard, CandidatesGuard, ChangeKind, FileOfferingStore, OfferingStore, Offerings,
    OfferingsChanged,
};
pub use orchestration::{NourishmentOrchestration, NurturingOrchestration};
pub use reconciliation::{ReconciliationResult, reconcile_services};
pub use registry::Registry;
pub use service_discovery::{
    FoundService, ServiceDiscoveryResponse, ServiceSearchCriteria, StoneRef, find_services,
    get_offering_port, list_all_local_services,
};
pub use storage::{
    Management, Media, Medium, Storage, Volume, VolumeIngestor, VolumeState, Volumes, new_media,
    new_volumes,
};
pub use storage_service::{LocalStorage, ProxyTarget, StorageRoute};
// Re-export TopologyEntry from common (now shared type)
pub use capabilities::{CapabilityExecutor, CapabilityMutationResult, Executor};
pub use ceremony::{
    Ceremony, CeremonyId, CeremonyInitiator, CeremonyOptions, CeremonyRegistry, CeremonyState,
    CeremonyType, Phase, PhaseState, execute_nourish_offering,
};
pub use companion::Companion;
pub use current::{Current, Stone as CurrentStone};
pub use discovery::{Discovery, DiscoveryChangeKind, DiscoveryChanged};
pub use events::{DomainEvent, OfferingEvent, PondEvent, StoneEvent, StorageEvent};
pub use harvest::{HarvestId, HarvestManifest, VolumeArchive};
pub use infrastructure::{
    DockerRegistry, InfrastructureHandler, InfrastructureHandlerRegistry, OfferingInstance,
};
pub use jobs::{
    DEFAULT_TERMINAL_TTL as JOBS_DEFAULT_TERMINAL_TTL, EvictionReason as JobsEvictionReason, Job,
    JobStatus, Jobs, JobsChangeKind, JobsChanged, ReapReport as JobsReapReport,
};
pub use network::{
    NetworkError, NetworkMode, PoolExhausted, ProbeResult, StaticIpActive, StaticIpDesired,
    StaticIpRelease, StaticIpRequest, StaticIpSeverity, StaticIpState,
};
pub use nurturing::{
    NurturingIndex, NurturingResult, NurturingSlot, NurturingSnapshot, OfferingSlots,
    snapshot_from_harvest,
};
pub use platform::Platform;
pub use pond::{PondMetadata, load_pond_metadata, save_pond_metadata};
pub use presence::Presence;
pub use security::{
    CeremonyPersistence, PondClient, Security, SecurityChangeKind, SecurityChanged,
};
pub use subsystems::{SubsystemStatus, Subsystems, SubsystemsChangeKind, SubsystemsChanged};
pub use tool::{
    EntryOrigin, GardenRegistry, GardenRegistryInner, RegistryEntry, Tool, ToolQuery,
    ToolsSnapshotPayload, new_registry, stream_event_type_for_delta,
};
// Categories are now data-driven via garden_common::manifests::get_category_registry()
