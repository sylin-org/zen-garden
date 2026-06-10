//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod api_manifest;
pub mod api_utils;
pub mod client;
pub mod command_manifest;
pub mod companion;
pub mod compatibility;
pub mod console;
pub mod constants;
pub mod detection;
pub mod discovery;
pub mod domain;
pub mod host;
pub mod http;
pub mod election;
pub mod errors;
pub mod events;
pub mod firefly_roster;
pub mod infra;
pub mod jobs;
pub mod manifests;
pub mod mdns;
pub mod notifications;
pub mod nourishment;
pub mod nurturing;
pub mod offerings;
pub mod persistence;
pub mod platform_runtime;
pub mod presence;
pub mod resources;
pub mod responses;
pub mod stone;
pub mod storage;
pub mod templates;
pub mod tools;
pub mod traits;
pub mod types;
pub mod uri;
pub mod utils;

pub use platform_runtime::PlatformRuntime;

// Canonical stone value objects (ARCH-0003 Wave 1b)
pub use stone::{Current, Environment, OsKind, Stone};
// Canonical companion value objects (ARCH-0003 Wave 1b)
pub use companion::{Companion, Manifest};

// ── Explicit re-exports (narrowed from wildcard dumps) ──────────────

pub use client::{GardenApiResponse, GardenHttpClient, StoneApi, StoneApiError};
pub use types::peer_address::PeerAddress;
pub use types::topology::TopologyEntry;
pub use utils::{format_bytes, format_uptime};

// Types — explicit high-frequency re-exports (was: pub use types::*)
pub use types::{
    AdoptedControlLevel,
    AdoptedData,
    AiCapabilitiesSummary,
    // Error
    ApiError,
    BorrowedData,
    CapabilityCollection,
    CapabilityDisplay,
    CapabilityItem,
    CompatibilityRule,
    // Compatibility
    CompatibilityRules,
    ComponentHealth,
    ConfigPatch,
    ContainerResources,
    // Orchestration
    CoordinationMode,
    CpuCapabilities,
    CpuResources,
    // Health
    DaemonHealthStatus,
    DetectionStatus,
    // Discovery
    DiscoveryRequest,
    DiscoveryResponse,
    DiskCapabilities,
    DiskResources,
    DiskType,
    ErrorDetails,
    FallbackConfig,
    GardenEvent,
    GatewayRegistration,
    GpuInfo,
    GuidanceFrontmatter,
    GuidanceTrigger,
    // Hardware
    HardwareCapabilities,
    HardwareInventory,
    HealthCheck,
    HealthMethod,
    HealthcheckPattern,
    InterfaceResources,
    KeystoneRequest,
    LanternServiceState,
    LanternStoneState,
    LanternTopology,
    ManagedData,
    MemoryCapabilities,
    MemoryResources,
    NetworkResources,
    // Offering
    Offering,
    OfferingGuidance,
    OfferingLocation,
    OfferingMode,
    OfferingModeData,
    OfferingRole,
    OfferingStatus,
    OrchestrationState,
    PlaceStoneRequest,
    // Pond
    PondConfig,
    PortConflictHandler,
    PortRemediation,
    Ports,
    PostInstallHealthcheck,
    // Lantern
    RegisterRequest,
    RegisterResponse,
    RegisterServiceInfo,
    RemediationFile,
    ResolveRequest,
    ResolveResponse,
    ResolveServiceInfo,
    ResourcesSnapshot,
    RuntimeInfo,
    ScheduledTask,
    ServiceHealthStatus,
    ServiceInfo,
    // Service
    ServiceStatus,
    StoneGoodbyePayload,
    StoneInviteRequest,
    StoneInviteResponse,
    StoneResources,
    StoneStatus,
    StorageResources,
    SubCapability,
    // Task
    TaskAction,
    TaskCategory,
    TaskDefinition,
    TaskResult,
    TopologyServiceEntry,
    UdpAnnouncement,
    WellKnownPort,
    // Ports catalog
    WellKnownPortsCatalog,
};
