//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod api_manifest;
pub mod api_utils;
pub mod cli;
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
pub mod presence;
pub mod responses;
pub mod storage;
pub mod stone;
pub mod templates;
pub mod tools;
pub mod traits;
pub mod types;
pub mod platform_runtime;
pub mod utils;

pub use platform_runtime::PlatformRuntime;

// Canonical stone value objects (ARCH-0003 Wave 1b)
pub use stone::{Current, Environment, OsKind, Stone};
// Canonical companion value objects (ARCH-0003 Wave 1b)
pub use companion::{Companion, Manifest};

// ── Explicit re-exports (narrowed from wildcard dumps) ──────────────

pub use client::{GardenApiResponse, GardenHttpClient};
pub use types::peer_address::PeerAddress;
pub use types::topology::TopologyEntry;
pub use utils::{format_bytes, format_uptime};

// Types — explicit high-frequency re-exports (was: pub use types::*)
pub use types::{
    // Service
    ServiceStatus, ServiceInfo, ServiceHealthStatus, Ports, SubCapability,
    CapabilityItem, CapabilityCollection, CapabilityDisplay,
    // Offering
    Offering, OfferingStatus, OfferingMode, OfferingModeData, OfferingLocation,
    ManagedData, AdoptedData, BorrowedData, ConfigPatch,
    AdoptedControlLevel, HealthMethod,
    OfferingGuidance, GuidanceFrontmatter, GuidanceTrigger,
    // Orchestration
    CoordinationMode, OfferingRole, OrchestrationState,
    // Hardware
    HardwareCapabilities, HardwareInventory, StoneResources, ContainerResources,
    GpuInfo, DetectionStatus, NetworkMetrics, DiskType, StorageMetrics,
    CpuCapabilities, MemoryCapabilities, DiskCapabilities,
    MetricsSnapshot, CpuMetrics, MemoryMetrics, DiskMetrics, InterfaceMetrics,
    AiRuntime, AiCapabilitiesSummary, RuntimeInfo,
    // Discovery
    DiscoveryRequest, DiscoveryResponse, UdpAnnouncement,
    TopologyServiceEntry, StoneStatus, GatewayRegistration, StoneGoodbyePayload,
    // Health
    DaemonHealthStatus, HealthCheck, ComponentHealth,
    // Compatibility
    CompatibilityRules, CompatibilityRule, RuleCondition, FallbackConfig,
    PostInstallHealthcheck, HealthcheckPattern,
    // Pond
    PondConfig, KeystoneRequest, StoneInviteRequest, StoneInviteResponse, PlaceStoneRequest,
    // Lantern
    RegisterRequest, RegisterServiceInfo, RegisterResponse,
    ResolveRequest, ResolveResponse, ResolveServiceInfo,
    LanternTopology, LanternStoneState, LanternServiceState, GardenEvent,
    // Ports catalog
    WellKnownPortsCatalog, WellKnownPort, PortConflictHandler, PortRemediation, RemediationFile,
    // Task
    TaskCategory, TaskDefinition, ScheduledTask, TaskResult,
    // Error
    ApiError, ErrorDetails,
};

