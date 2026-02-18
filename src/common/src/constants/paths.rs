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

/// Get shared data directory for cross-process data
///
/// A stable, absolute, system-wide location for data shared between Moss and
/// other processes (Koan clients, containers). Unlike `data_dir()` which may
/// be relative on Windows (`.zen-garden`), this always resolves to an absolute
/// path so external processes can locate it by well-known convention.
///
/// | Platform | Default |
/// |----------|---------|
/// | Linux    | `/var/lib/zen-garden` (same as `data_dir()`) |
/// | Windows  | `{ProgramData}\zen-garden` |
///
/// Override: `GARDEN_SHARED_DATA_DIR` environment variable.
pub fn shared_data_dir() -> String {
    std::env::var("GARDEN_SHARED_DATA_DIR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        {
            let program_data =
                std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
            format!(r"{}\zen-garden", program_data)
        }
        #[cfg(not(target_os = "windows"))]
        {
            data_dir()
        }
    })
}

/// Get harvest storage directory
pub fn harvest_dir() -> String {
    std::env::var("GARDEN_HARVEST_DIR").unwrap_or_else(|_| format!("{}/harvests", data_dir()))
}

/// Get container volumes directory
/// Used for Docker volume mounts (data stored per-offering)
pub fn volumes_dir() -> String {
    std::env::var("GARDEN_VOLUMES_DIR").unwrap_or_else(|_| format!("{}/volumes", data_dir()))
}

/// Get shared topology directory for cross-process topology sharing
/// Layout: {shared_data_dir}/topology/
///
/// Contains two files with distinct ownership:
/// - `garden-topology.json` (written by Moss — authoritative mesh snapshot)
/// - `garden-stones.json` (written by clients — operational roster)
///
/// Exposed to managed containers via bind mount at `/app/cache/zen-garden/`.
pub fn topology_dir() -> String {
    format!("{}/topology", shared_data_dir())
}

/// Container-side path for the shared topology directory
/// Matches Koan.ZenGarden's StoneRosterPathResolver convention
pub const CONTAINER_TOPOLOGY_DIR: &str = "/app/cache/zen-garden";

/// Topology file written by Moss (authoritative mesh snapshot)
pub const TOPOLOGY_FILE: &str = "garden-topology.json";

/// Get jobs persistence file path
pub fn jobs_file() -> String {
    std::env::var("GARDEN_JOBS_FILE").unwrap_or_else(|_| format!("{}/jobs.json", data_dir()))
}

/// Get stored offerings directory (portable backups)
pub fn stored_dir() -> String {
    std::env::var("GARDEN_STORED_DIR").unwrap_or_else(|_| format!("{}/stored", data_dir()))
}

/// Get ceremony journal directory
pub fn ceremony_journal_dir() -> String {
    std::env::var("GARDEN_CEREMONY_DIR").unwrap_or_else(|_| format!("{}/ceremonies", data_dir()))
}

/// Get staging directory for package deployments
pub fn staging_dir() -> String {
    std::env::var("GARDEN_STAGING_DIR").unwrap_or_else(|_| format!("{}/staging", data_dir()))
}

/// Get Companions/services directory
/// Contains subdirectories with Companion executables
pub fn companions_dir() -> String {
    std::env::var("GARDEN_companions_dir").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        {
            ".zen-garden\\Companions".to_string()
        }
        #[cfg(not(target_os = "windows"))]
        {
            "/usr/local/bin/companions".to_string()
        }
    })
}

/// Get logs directory for daemon file logging
/// Layout: {data_dir}/logs/
pub fn logs_dir() -> String {
    std::env::var("ZG_LOGS_DIR").unwrap_or_else(|_| format!("{}/logs", data_dir()))
}

/// Get pond metadata file path
/// Layout: {data_dir}/pond.json
pub fn pond_metadata_file() -> String {
    format!("{}/pond.json", data_dir())
}

// ============================================================================
// Linux-Specific Paths (for SSH validation from Windows to Linux stones)
// ============================================================================

/// Linux config directory (always /etc/zen-garden, regardless of compile target)
pub const LINUX_CONFIG_DIR: &str = "/etc/zen-garden";

/// Linux data directory (always /var/lib/zen-garden, regardless of compile target)
pub const LINUX_DATA_DIR: &str = "/var/lib/zen-garden";

/// Linux harvest directory
pub fn linux_harvest_dir() -> String {
    format!("{}/harvests", LINUX_DATA_DIR)
}

/// Linux nurturing index directory
pub fn linux_nurturing_index_dir() -> String {
    format!("{}/nurturing", LINUX_CONFIG_DIR)
}

/// Linux nurturing index file path
pub fn linux_nurturing_index_path() -> String {
    format!("{}/index.json", linux_nurturing_index_dir())
}

// ============================================================================
// Nurturing Paths
// ============================================================================

/// Nurturing sub-directory name within config/data directories
pub const NURTURING_SUBDIR: &str = "nurturing";

/// Nurturing index filename
pub const NURTURING_INDEX_FILE: &str = "index.json";

/// Get nurturing index directory (local A/B slot metadata)
/// Layout: {config_dir}/nurturing/
pub fn nurturing_index_dir() -> String {
    format!("{}/{}", config_dir(), NURTURING_SUBDIR)
}

/// Get nurturing index file path
/// Layout: {config_dir}/nurturing/index.json
pub fn nurturing_index_path() -> String {
    format!("{}/{}", nurturing_index_dir(), NURTURING_INDEX_FILE)
}

// ============================================================================
// Zen Garden Sentinel (device protection marker)
// ============================================================================

/// Sentinel filename placed on Zen Garden storage media to prevent accidental
/// formatting during automated OS installation or disk operations.
///
/// Any block device whose filesystem contains this file at its root will be
/// skipped by the NewStone preseed disk-selection logic. Companions that expose
/// storage (Firefly, etc.) should also write this file to their media.
///
/// Contents: JSON object with at least `{"role":"<component>"}`.
pub const ZEN_GARDEN_SENTINEL: &str = ".zen-garden-sentinel";

// ============================================================================
// Seed Bank Memories & Storage Paths
// ============================================================================

/// Seed bank garden root directory
/// Layout: {mount_path}/garden/
pub const SEED_BANK_GARDEN_DIR: &str = "garden";

/// Seed bank memories directory (nurturing backups)
/// Layout: {mount_path}/garden/memories/
pub const SEED_BANK_MEMORIES_DIR: &str = "garden/memories";

/// Seed bank memories index filename
/// Layout: {mount_path}/garden/memories/index.json
pub const SEED_BANK_MEMORIES_INDEX_FILE: &str = "index.json";

/// Seed bank memories offering manifest filename
/// Layout: {mount_path}/garden/memories/{offering_id}/offering.json
pub const SEED_BANK_MEMORIES_OFFERING_MANIFEST_FILE: &str = "offering.json";

/// Seed bank storage directory (S3 root)
/// Layout: {mount_path}/garden/storage/
pub const SEED_BANK_STORAGE_DIR: &str = "garden/storage";

/// Get memories directory on a seed bank
/// Layout: {mount_path}/garden/memories/
pub fn seed_bank_memories_dir(mount_path: &str) -> String {
    format!("{}/{}", mount_path, SEED_BANK_MEMORIES_DIR)
}

/// Get memories index path on a seed bank
/// Layout: {mount_path}/garden/memories/index.json
pub fn seed_bank_memories_index_path(mount_path: &str) -> String {
    format!(
        "{}/{}",
        seed_bank_memories_dir(mount_path),
        SEED_BANK_MEMORIES_INDEX_FILE
    )
}

/// Get offering memories directory on a seed bank
/// Layout: {mount_path}/garden/memories/{offering_id}/
pub fn seed_bank_memory_offering_dir(mount_path: &str, offering_id: &str) -> String {
    format!("{}/{}", seed_bank_memories_dir(mount_path), offering_id)
}

/// Get offering manifest path on a seed bank
/// Layout: {mount_path}/garden/memories/{offering_id}/offering.json
pub fn seed_bank_memory_offering_manifest_path(mount_path: &str, offering_id: &str) -> String {
    format!(
        "{}/{}",
        seed_bank_memory_offering_dir(mount_path, offering_id),
        SEED_BANK_MEMORIES_OFFERING_MANIFEST_FILE
    )
}

/// Get harvest tarball path on a seed bank
/// Layout: {mount_path}/garden/memories/{offering_id}/{harvest_id}.tar.gz
pub fn seed_bank_memory_harvest_path(
    mount_path: &str,
    offering_id: &str,
    harvest_id: &str,
) -> String {
    format!(
        "{}/{}.tar.gz",
        seed_bank_memory_offering_dir(mount_path, offering_id),
        harvest_id
    )
}

/// Get storage directory on a seed bank
/// Layout: {mount_path}/garden/storage/
pub fn seed_bank_storage_dir(mount_path: &str) -> String {
    format!("{}/{}", mount_path, SEED_BANK_STORAGE_DIR)
}

// ========================================================================
// Audit Log Paths
// ========================================================================

/// Audit log filename
pub const AUDIT_LOG_FILE: &str = "audit.log";

/// Audit log path
/// Layout: {data_dir}/audit.log
pub fn audit_log_path() -> String {
    format!("{}/{}", data_dir(), AUDIT_LOG_FILE)
}
