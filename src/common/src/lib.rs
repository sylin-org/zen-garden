//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod api_manifest;
pub mod api_utils;
pub mod audit;
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
pub mod naming;
pub mod net;
pub mod notifications;
pub mod nourishment;
pub mod offerings;
pub mod persistence;
pub mod presence;
pub mod responses;
pub mod storage;
pub mod templates;
pub mod tools;
pub mod traits;
pub mod types;
pub mod ui;
pub mod platform_runtime;
pub mod utils;

pub use platform_runtime::PlatformRuntime;

// Re-export commonly used items
pub use audit::{log_access, AuditAccessEntry};
pub use cli_colors::{AnsiColor, CliFormatter, ColorSupport};
pub use client::{GardenApiResponse, GardenHttpClient};
pub use jobs::*;
pub use responses::*;
pub use types::peer_address::PeerAddress;
pub use types::topology::TopologyEntry;
pub use types::*;
pub use utils::*;

// Re-export health and vitality constants for easy access
pub use constants::{
    AUTH_BEARER_PREFIX, CHECK_FAIL, CHECK_PASS, CHECK_WARN, COMPAT_FAIL, COMPAT_FALLBACK,
    COMPAT_PASS, COMPAT_WARNING, DEFAULT_STONE_NAME, ENDPOINT_CAPABILITIES, ENDPOINT_HEALTH,
    ENV_GARDEN_STONE, ENV_GARDEN_UNICODE, ENV_LANTERN_ENDPOINT, ENV_NO_COLOR, ENV_STONE_HOST,
    ENV_STONE_NAME, HEADER_AUTHORIZATION, HEALTH_DEGRADED, HEALTH_HEALTHY, HEALTH_UNHEALTHY,
    SERVICE_DEGRADED, SERVICE_MAINTENANCE, SERVICE_RUNNING, SERVICE_STOPPED, SERVICE_UNKNOWN,
    STATUS_COMPLETED, STATUS_ERROR, STATUS_FAILED, STATUS_SUCCESS, VALUE_UNKNOWN, VITALITY_DORMANT,
    VITALITY_NEEDS_ATTENTION, VITALITY_THRIVING, VITALITY_WITHERING,
};

// Re-export offering lifecycle event constants
pub use constants::{
    EVENT_DEPLOYED, EVENT_DESTROYED, EVENT_HEALTH_CHANGED, EVENT_REMOVED, EVENT_RENAMED,
    EVENT_ROLE_CHANGED, EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED,
};

// Re-export announcement type constants
pub use constants::{
    ANNOUNCEMENT_STONE_CHIRP, ANNOUNCEMENT_STONE_GOODBYE, ANNOUNCEMENT_STORAGE_DETECTED,
    ANNOUNCEMENT_STORAGE_REMOVED,
};

// Re-export SSE event level constants
pub use constants::{SSE_LEVEL_DEBUG, SSE_LEVEL_ERROR, SSE_LEVEL_INFO, SSE_LEVEL_WARN};

// Re-export notification types and constants
pub use notifications::{
    NotificationRegistry, NotificationTag, NOTIF_SOURCE_ADOPTED_OFFLINE, NOTIF_SOURCE_CANDIDATES,
    NOTIF_SOURCE_COMPANION_CRASHED, NOTIF_SOURCE_COMPANION_NEW, NOTIF_SOURCE_NOURISHMENT,
    NOTIF_SOURCE_OFFERINGS_DEGRADED, NOTIF_SOURCE_ORPHAN_CONTAINERS, NOTIF_SOURCE_STORAGE_OFFLINE,
    NOTIF_SOURCE_SYSTEM_CRITICAL, TAG_ATTENTION, TAG_OPPORTUNITY,
};
