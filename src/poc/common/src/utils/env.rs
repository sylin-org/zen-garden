//! Environment variable utilities
//!
//! Centralized, typed access to environment variables with
//! validation and consistent fallback behavior.

use std::env;

/// Get environment variable with typed default
pub fn get_var_or<T: From<String>>(key: &str, default: T) -> T {
    env::var(key).ok().map(T::from).unwrap_or(default)
}

/// Get optional environment variable
pub fn get_var_opt(key: &str) -> Option<String> {
    env::var(key).ok()
}

/// Check if environment variable is set (regardless of value)
pub fn has_var(key: &str) -> bool {
    env::var(key).is_ok()
}

/// Environment variable keys (centralized registry)
pub mod keys {
    // Data paths
    pub const DATA_DIR: &str = "GARDEN_DATA_DIR";
    pub const CONFIG_DIR: &str = "GARDEN_CONFIG_DIR";
    pub const HARVEST_DIR: &str = "GARDEN_HARVEST_DIR";
    pub const STAGING_DIR: &str = "GARDEN_STAGING_DIR";
    pub const STORED_DIR: &str = "GARDEN_STORED_DIR";

    // Stone configuration
    pub const STONE_NAME: &str = "GARDEN_STONE_NAME";
    pub const STONE_HOST: &str = "GARDEN_STONE_HOST";
    pub const STONE_HOME: &str = "GARDEN_STONE_HOME";
    pub const STONE_USER: &str = "GARDEN_STONE_USER";
    pub const FIRST_RUN_FLAG: &str = "GARDEN_FIRST_RUN_FLAG";

    // Endpoints
    pub const GARDEN_STONE: &str = "GARDEN_STONE";
    pub const LANTERN_ENDPOINT: &str = "LANTERN_ENDPOINT";

    // Runtime flags
    pub const NO_COLOR: &str = "NO_COLOR";
    pub const GARDEN_NO_COLOR: &str = "GARDEN_NO_COLOR";
    pub const GARDEN_UNICODE: &str = "GARDEN_UNICODE";
    pub const GARDEN_QUIET: &str = "GARDEN_QUIET";
    pub const RUNNING_AS_SERVICE: &str = "RUNNING_AS_SERVICE";
    pub const ZEN_GARDEN_CONTAINER: &str = "ZEN_GARDEN_CONTAINER";

    // External tools
    pub const CUDA_PATH: &str = "CUDA_PATH";
    pub const SYSTEM_ROOT: &str = "SystemRoot";
    pub const INTEL_OPENVINO_DIR: &str = "INTEL_OPENVINO_DIR";
    pub const PROGRAMDATA: &str = "PROGRAMDATA";
    pub const HOME: &str = "HOME";
}

/// Typed environment configuration
pub struct EnvConfig;

impl EnvConfig {
    // Path accessors
    pub fn data_dir() -> Option<String> {
        get_var_opt(keys::DATA_DIR)
    }

    pub fn config_dir() -> Option<String> {
        get_var_opt(keys::CONFIG_DIR)
    }

    pub fn staging_dir() -> Option<String> {
        get_var_opt(keys::STAGING_DIR)
    }

    pub fn harvest_dir() -> Option<String> {
        get_var_opt(keys::HARVEST_DIR)
    }

    pub fn stored_dir() -> Option<String> {
        get_var_opt(keys::STORED_DIR)
    }

    // Stone configuration
    pub fn stone_name() -> Option<String> {
        get_var_opt(keys::STONE_NAME)
    }

    pub fn stone_endpoint() -> Option<String> {
        get_var_opt(keys::GARDEN_STONE)
    }

    pub fn lantern_endpoint() -> Option<String> {
        get_var_opt(keys::LANTERN_ENDPOINT)
    }

    // Flags
    pub fn is_no_color() -> bool {
        has_var(keys::NO_COLOR) || has_var(keys::GARDEN_NO_COLOR)
    }

    pub fn is_unicode_enabled() -> bool {
        has_var(keys::GARDEN_UNICODE)
    }

    pub fn is_quiet() -> bool {
        has_var(keys::GARDEN_QUIET)
    }

    pub fn is_running_as_service() -> bool {
        has_var(keys::RUNNING_AS_SERVICE)
    }

    pub fn is_containerized() -> bool {
        has_var(keys::ZEN_GARDEN_CONTAINER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_var_or() {
        // Should return default when var not set
        let result = get_var_or("NONEXISTENT_VAR_12345", "default".to_string());
        assert_eq!(result, "default");
    }

    #[test]
    fn test_get_var_opt() {
        // Should return None when var not set
        assert!(get_var_opt("NONEXISTENT_VAR_12345").is_none());
    }

    #[test]
    fn test_has_var() {
        // Should return false when var not set
        assert!(!has_var("NONEXISTENT_VAR_12345"));

        // SAFETY: Test-only; no concurrent env var access in this test.
        unsafe {
            env::set_var("TEST_VAR_12345", "value");
        }
        assert!(has_var("TEST_VAR_12345"));
        unsafe {
            env::remove_var("TEST_VAR_12345");
        }
    }

    #[test]
    fn test_env_config_flags() {
        // SAFETY: Test-only; no concurrent env var access in this test.
        unsafe {
            env::set_var(keys::NO_COLOR, "1");
        }
        assert!(EnvConfig::is_no_color());
        unsafe {
            env::remove_var(keys::NO_COLOR);
        }

        unsafe {
            env::set_var(keys::GARDEN_NO_COLOR, "1");
        }
        assert!(EnvConfig::is_no_color());
        unsafe {
            env::remove_var(keys::GARDEN_NO_COLOR);
        }

        unsafe {
            env::set_var(keys::GARDEN_QUIET, "1");
        }
        assert!(EnvConfig::is_quiet());
        unsafe {
            env::remove_var(keys::GARDEN_QUIET);
        }
    }
}
