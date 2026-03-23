//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod api_manifest;
pub mod api_utils;
pub mod client;
pub mod command_manifest;
pub mod companion;
pub mod console;
pub mod constants;
pub mod detection;
pub mod discovery;
pub mod election;
pub mod errors;
pub mod events;
pub mod infra;
pub mod jobs;
pub mod manifests;
pub mod mdns;
pub mod metrics;
pub mod notifications;
pub mod nourishment;
pub mod nurturing;
pub mod offerings;
pub mod persistence;
pub mod platform_runtime;
pub mod presence;
pub mod responses;
pub mod stone;
pub mod storage;
pub mod templates;
pub mod tools;
pub mod traits;
pub mod types;
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
    AiRuntime,
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
    CpuMetrics,
    // Health
    DaemonHealthStatus,
    DetectionStatus,
    // Discovery
    DiscoveryRequest,
    DiscoveryResponse,
    DiskCapabilities,
    DiskMetrics,
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
    InterfaceMetrics,
    KeystoneRequest,
    LanternServiceState,
    LanternStoneState,
    LanternTopology,
    ManagedData,
    MemoryCapabilities,
    MemoryMetrics,
    MetricsSnapshot,
    NetworkMetrics,
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
    RuleCondition,
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
    StorageMetrics,
    SubCapability,
    // Task
    TaskCategory,
    TaskDefinition,
    TaskResult,
    TopologyServiceEntry,
    UdpAnnouncement,
    WellKnownPort,
    // Ports catalog
    WellKnownPortsCatalog,
};
