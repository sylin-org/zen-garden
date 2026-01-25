//! Path Constants
//! File system paths with GARDEN_ environment variable overrides

/// Get config directory (default: /etc/zen-garden)
pub fn config_dir() -> String {
    std::env::var("GARDEN_CONFIG_DIR").unwrap_or_else(|_| "/etc/zen-garden".to_string())
}

/// Get stone home directory (default: /home/stone)
pub fn stone_home() -> String {
    std::env::var("GARDEN_STONE_HOME").unwrap_or_else(|_| "/home/stone".to_string())
}

/// Get first-run flag path (default: /etc/zen-garden/.first-run-complete)
pub fn first_run_flag() -> String {
    std::env::var("GARDEN_FIRST_RUN_FLAG")
        .unwrap_or_else(|_| "/etc/zen-garden/.first-run-complete".to_string())
}

/// Get stone username (default: stone)
pub fn stone_user() -> String {
    std::env::var("GARDEN_STONE_USER").unwrap_or_else(|_| "stone".to_string())
}

/// Get data directory (default: /var/lib/zen-garden on Linux, .zen-garden on Windows)
pub fn data_dir() -> String {
    std::env::var("GARDEN_DATA_DIR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        {
            ".zen-garden".to_string()
        }
        #[cfg(not(target_os = "windows"))]
        {
            "/var/lib/zen-garden".to_string()
        }
    })
}

/// Get harvest storage directory
pub fn harvest_dir() -> String {
    std::env::var("GARDEN_HARVEST_DIR").unwrap_or_else(|_| {
        format!("{}/harvests", data_dir())
    })
}

/// Get stored offerings directory (portable backups)
pub fn stored_dir() -> String {
    std::env::var("GARDEN_STORED_DIR").unwrap_or_else(|_| {
        format!("{}/stored", data_dir())
    })
}

/// Get ceremony journal directory
pub fn ceremony_journal_dir() -> String {
    std::env::var("GARDEN_CEREMONY_DIR").unwrap_or_else(|_| {
        format!("{}/ceremonies", data_dir())
    })
}
