//! Zen Common Library
//! Shared types, networking, errors, utilities, constants, responses, and jobs for Zen Garden

pub mod types;
pub mod net;
pub mod errors;
pub mod utils;
pub mod constants;
pub mod responses;
pub mod jobs;
pub mod client;
pub mod cli;
pub mod traits;
pub mod events;
pub mod persistence;
pub mod api_utils;
pub mod manifests;
pub mod cli_colors;
pub mod nourishment;
pub mod election;
pub mod infra;
pub mod metrics;
pub mod ui;
pub mod detection;
pub mod console;
pub mod presence;
pub mod companion;
pub mod command_manifest;
pub mod offerings;
pub mod api_manifest;
pub mod storage;
pub mod templates;

// Re-export commonly used items
pub use types::*;
pub use types::topology::TopologyEntry;
pub use utils::*;
pub use responses::*;
pub use jobs::*;
pub use client::{GardenHttpClient, GardenApiResponse};
pub use cli_colors::{CliFormatter, AnsiColor, ColorSupport};

// Re-export health and vitality constants for easy access
pub use constants::{
    HEALTH_HEALTHY, HEALTH_DEGRADED, HEALTH_UNHEALTHY,
    CHECK_PASS, CHECK_WARN, CHECK_FAIL,
    COMPAT_PASS, COMPAT_FALLBACK, COMPAT_WARNING, COMPAT_FAIL,
    VITALITY_THRIVING, VITALITY_NEEDS_ATTENTION, VITALITY_WITHERING, VITALITY_DORMANT,
    SERVICE_RUNNING, SERVICE_STOPPED, SERVICE_MAINTENANCE, SERVICE_DEGRADED, SERVICE_UNKNOWN,
    ENV_GARDEN_STONE, ENV_STONE_NAME, ENV_STONE_HOST, ENV_LANTERN_ENDPOINT,
    ENV_NO_COLOR, ENV_GARDEN_UNICODE,
    VALUE_UNKNOWN, DEFAULT_STONE_NAME,
    STATUS_COMPLETED, STATUS_SUCCESS, STATUS_FAILED, STATUS_ERROR,
    HEADER_AUTHORIZATION, AUTH_BEARER_PREFIX,
    ENDPOINT_HEALTH, ENDPOINT_CAPABILITIES,
};

// Re-export offering lifecycle event constants
pub use constants::{
    EVENT_DEPLOYED, EVENT_STARTED, EVENT_STOPPED, EVENT_REMOVED,
    EVENT_DESTROYED, EVENT_UPDATED, EVENT_RENAMED, EVENT_HEALTH_CHANGED,
};

// Re-export announcement type constants
pub use constants::{
    ANNOUNCEMENT_STONE_CHIRP, ANNOUNCEMENT_STONE_GOODBYE,
    ANNOUNCEMENT_STORAGE_DETECTED, ANNOUNCEMENT_STORAGE_REMOVED,
};

// Re-export SSE event level constants
pub use constants::{
    SSE_LEVEL_INFO, SSE_LEVEL_WARN, SSE_LEVEL_ERROR, SSE_LEVEL_DEBUG,
};

// Compatibility aliases for old code (to be removed during Phase 3 refactoring)
pub mod ports {
    pub use crate::constants::{DISCOVERY_UDP, MOSS_HTTP, LANTERN_HTTP};
}

pub mod names {
    pub use crate::constants::{
        MOSS_BINARY, RAKE_BINARY, LANTERN_BINARY,
        MOSS_CONFIG, LANTERN_CONFIG,
        MOSS_SERVICE, LANTERN_SERVICE,
        CONFIG_DIR, STONE_USER, STONE_HOME, FIRST_RUN_FLAG,
    };
}

pub mod error_codes {
    pub use crate::constants::{
        INVALID_REQUEST, TEMPLATE_NOT_FOUND, CONTAINER_NOT_RUNNING, INVALID_COMPONENT,
        SERVICE_NOT_FOUND, OFFERING_NOT_FOUND, NOT_FOUND, JOB_NOT_FOUND,
        DOCKER_ERROR, INTERNAL_ERROR, REMOVE_FAILED, TEMPLATE_LOAD_FAILED,
        UPGRADE_FAILED, INSUFFICIENT_RESOURCES,
        DOCKER_UNAVAILABLE,
        COMPATIBILITY_FAILED,
    };
}

/// Path functions for standard directories
pub mod paths {
    pub use crate::constants::paths::{
        config_dir, stone_home, first_run_flag, stone_user,
        data_dir, harvest_dir, stored_dir, ceremony_journal_dir,
        // Linux-specific paths (for SSH validation)
        LINUX_CONFIG_DIR, LINUX_DATA_DIR,
        linux_harvest_dir, linux_nurturing_index_dir, linux_nurturing_index_path,
        // Nurturing paths
        nurturing_index_dir, nurturing_index_path,
        seed_bank_nurturing_dir, seed_bank_offering_dir, seed_bank_harvest_path,
        NURTURING_SUBDIR, NURTURING_INDEX_FILE, SEED_BANK_APPS_DIR,
    };
}

