//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod api_manifest;
pub mod api_utils;
pub mod cli;
pub mod cli_colors;
pub mod client;
pub mod command_manifest;
pub mod companion;
pub mod console;
pub mod constants;
pub mod detection;
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
pub mod ui;
pub mod platform_runtime;
pub mod utils;

pub use platform_runtime::PlatformRuntime;

// Canonical stone value objects (ARCH-0003 Wave 1b)
pub use stone::{Current, Environment, OsKind, Stone};
// Canonical companion value objects (ARCH-0003 Wave 1b)
pub use companion::{Companion, Manifest};

// ── Explicit re-exports (narrowed from wildcard dumps) ──────────────

pub use cli_colors::{AnsiColor, CliFormatter, ColorSupport};
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

// Constants — explicit high-frequency re-exports (consumers should
// prefer garden_common::constants::NAME for new code)
pub use constants::{
    SERVICE_RUNNING, SERVICE_STOPPED, SERVICE_DEGRADED, SERVICE_MAINTENANCE, SERVICE_UNKNOWN,
    HEALTH_HEALTHY, HEALTH_DEGRADED, HEALTH_UNHEALTHY,
    CHECK_PASS, CHECK_FAIL, CHECK_WARN,
    COMPAT_PASS, COMPAT_FAIL, COMPAT_FALLBACK, COMPAT_WARNING,
    VITALITY_DORMANT, VITALITY_THRIVING, VITALITY_NEEDS_ATTENTION, VITALITY_WITHERING,
    STATUS_COMPLETED, STATUS_SUCCESS, STATUS_FAILED, STATUS_ERROR,
    VALUE_UNKNOWN, DEFAULT_STONE_NAME,
    AUTH_BEARER_PREFIX, HEADER_AUTHORIZATION,
    ENDPOINT_CAPABILITIES, ENDPOINT_HEALTH,
    ENV_GARDEN_STONE, ENV_GARDEN_UNICODE, ENV_LANTERN_ENDPOINT, ENV_NO_COLOR,
    ENV_STONE_HOST, ENV_STONE_NAME,
    SSE_LEVEL_DEBUG, SSE_LEVEL_ERROR, SSE_LEVEL_INFO, SSE_LEVEL_WARN,
};

// Offering lifecycle event constants
pub use constants::{
    EVENT_DEPLOYED, EVENT_DESTROYED, EVENT_HEALTH_CHANGED, EVENT_REMOVED, EVENT_RENAMED,
    EVENT_ROLE_CHANGED, EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED,
};

// Announcement type constants
pub use constants::{
    ANNOUNCEMENT_STONE_CHIRP, ANNOUNCEMENT_STONE_GOODBYE, ANNOUNCEMENT_STORAGE_DETECTED,
    ANNOUNCEMENT_STORAGE_REMOVED,
};

// Notification types
pub use notifications::{
    NotificationRegistry, NotificationTag, NOTIF_SOURCE_ADOPTED_OFFLINE, NOTIF_SOURCE_CANDIDATES,
    NOTIF_SOURCE_COMPANION_CRASHED, NOTIF_SOURCE_COMPANION_NEW, NOTIF_SOURCE_NOURISHMENT,
    NOTIF_SOURCE_OFFERINGS_DEGRADED, NOTIF_SOURCE_ORPHAN_CONTAINERS, NOTIF_SOURCE_STORAGE_OFFLINE,
    NOTIF_SOURCE_SYSTEM_CRITICAL, TAG_ATTENTION, TAG_OPPORTUNITY,
};
