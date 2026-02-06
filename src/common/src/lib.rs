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
pub mod metrics;
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
pub mod utils;

// Re-export commonly used items
pub use audit::{log_access, AuditAccessEntry};
pub use cli_colors::{AnsiColor, CliFormatter, ColorSupport};
pub use client::{GardenApiResponse, GardenHttpClient};
pub use jobs::*;
pub use responses::*;
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
    EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED,
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

// Compatibility aliases for old code (to be removed during Phase 3 refactoring)
pub mod ports {
    pub use crate::constants::{DISCOVERY_UDP, LANTERN_HTTP, MOSS_HTTP};
}

pub mod names {
    pub use crate::constants::{
        CONFIG_DIR, FIRST_RUN_FLAG, LANTERN_BINARY, LANTERN_CONFIG, LANTERN_SERVICE, MOSS_BINARY,
        MOSS_CONFIG, MOSS_SERVICE, RAKE_BINARY, STONE_HOME, STONE_USER,
    };
}

pub mod error_codes {
    pub use crate::constants::{
        COMPATIBILITY_FAILED, CONTAINER_NOT_RUNNING, DOCKER_ERROR, DOCKER_UNAVAILABLE,
        INSUFFICIENT_RESOURCES, INTERNAL_ERROR, INVALID_COMPONENT, INVALID_REQUEST, JOB_NOT_FOUND,
        NOT_FOUND, OFFERING_NOT_FOUND, REMOVE_FAILED, SERVICE_NOT_FOUND, TEMPLATE_LOAD_FAILED,
        TEMPLATE_NOT_FOUND, UPGRADE_FAILED,
    };
}

/// Path functions for standard directories
pub mod paths {
    pub use crate::constants::paths::{
        audit_log_path,
        ceremony_journal_dir,
        config_dir,
        data_dir,
        first_run_flag,
        harvest_dir,
        linux_harvest_dir,
        linux_nurturing_index_dir,
        linux_nurturing_index_path,
        // Nurturing paths
        nurturing_index_dir,
        nurturing_index_path,
        seed_bank_memories_dir,
        seed_bank_memories_index_path,
        seed_bank_memory_harvest_path,
        seed_bank_memory_offering_dir,
        seed_bank_memory_offering_manifest_path,
        seed_bank_storage_dir,
        stone_home,
        stone_user,
        stored_dir,
        AUDIT_LOG_FILE,
        // Linux-specific paths (for SSH validation)
        LINUX_CONFIG_DIR,
        LINUX_DATA_DIR,
        NURTURING_INDEX_FILE,
        NURTURING_SUBDIR,
        SEED_BANK_GARDEN_DIR,
        SEED_BANK_MEMORIES_DIR,
        SEED_BANK_MEMORIES_INDEX_FILE,
        SEED_BANK_MEMORIES_OFFERING_MANIFEST_FILE,
        SEED_BANK_STORAGE_DIR,
    };
}
