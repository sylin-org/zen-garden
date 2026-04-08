//! Storage health assessment (ARCH-0005 extraction).
//!
//! Domain-pure validation and health checks for seed banks.
//! No HTTP types — handlers map results to API responses.

use garden_common::constants::paths;
use garden_common::storage::StorageInfo;
use serde::Serialize;

// ============================================================================
// Types
// ============================================================================

/// Overall storage health report for this stone.
#[derive(Debug, Serialize)]
pub struct StorageHealth {
    pub ready: bool,
    pub bank_count: usize,
    pub ready_count: usize,
    pub banks: Vec<SeedBankHealth>,
    pub issues: Vec<String>,
}

/// Health status of a single seed bank.
#[derive(Debug, Serialize)]
pub struct SeedBankHealth {
    pub id: String,
    pub name: String,
    pub device: String,
    pub mount_path: String,
    pub canonical: bool,
    pub writable: bool,
    pub responsive: bool,
    pub io_errors: u64,
    pub ready: bool,
    pub issues: Vec<String>,
}

// ============================================================================
// Validation
// ============================================================================

/// Validate that a seed bank uses the canonical layout.
///
/// Checks for the required `.zen-garden/memories` and `.zen-garden/meta`
/// directories under the mount path.
pub fn validate_seed_bank_layout(mount_path: &str) -> Result<(), String> {
    let memories = std::path::Path::new(mount_path).join(paths::STORAGE_MEMORIES_DIR);
    let meta = std::path::Path::new(mount_path).join(paths::STORAGE_OBJECTS_META_DIR);

    let mut missing = Vec::new();
    if !memories.is_dir() {
        missing.push(paths::STORAGE_MEMORIES_DIR);
    }
    if !meta.is_dir() {
        missing.push(paths::STORAGE_OBJECTS_META_DIR);
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Seed bank is non-canonical; missing {}. Re-prepare the seed bank.",
            missing.join(" and ")
        ))
    }
}

/// Check whether a mount is read-only by reading `/proc/mounts`.
///
/// Returns `Some(true)` if read-only, `Some(false)` if read-write,
/// `None` if mount options are unavailable.
pub async fn is_mount_readonly(mount_path: &str) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let mounts = tokio::fs::read_to_string("/proc/mounts").await.ok()?;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[1] == mount_path {
                let opts = parts[3];
                let ro = opts.split(',').any(|o| o == "ro");
                return Some(ro);
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_path;
        Some(false)
    }
}

// ============================================================================
// Health assessment
// ============================================================================

/// Assess the health of all managed seed banks.
///
/// Checks each bank for canonical layout and writability, returning a
/// complete health report with per-bank and overall status.
pub async fn assess_storage_health(managed: Vec<StorageInfo>) -> StorageHealth {
    let mut banks = Vec::new();

    for bank in &managed {
        let mut issues = Vec::new();

        let canonical = validate_seed_bank_layout(&bank.mount_path).is_ok();
        if !canonical {
            issues.push("non-canonical layout".to_string());
        }

        let writable = match is_mount_readonly(&bank.mount_path).await {
            Some(true) => {
                issues.push("mount is read-only".to_string());
                false
            }
            Some(false) => true,
            None => {
                issues.push("mount options unavailable".to_string());
                false
            }
        };

        let ready = canonical && writable;

        banks.push(SeedBankHealth {
            id: bank.id.clone(),
            name: bank.name.clone(),
            device: bank.device.clone(),
            mount_path: bank.mount_path.clone(),
            canonical,
            writable,
            responsive: true, // assessed at query time — volume is mounted
            io_errors: 0,     // real-time monitoring via observe cycle (STORAGE-0018)
            ready,
            issues,
        });
    }

    let bank_count = banks.len();
    let ready_count = banks.iter().filter(|b| b.ready).count();
    let ready = ready_count > 0;

    let mut issues = Vec::new();
    if bank_count == 0 {
        issues.push("no seed banks mounted".to_string());
    } else if ready_count == 0 {
        issues.push("no seed banks are ready".to_string());
    }

    StorageHealth {
        ready,
        bank_count,
        ready_count,
        banks,
        issues,
    }
}
