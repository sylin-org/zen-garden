//! Hardware detection and capabilities management
//!
//! Provides composable functions for detecting system hardware,
//! managing capabilities cache, and progressive detection.
//!
//! The single capability detector is `tasks::hardware_detection::detect_capabilities_background`;
//! this module provides the cache I/O, the startup skeleton, and the DMI helpers it reuses.

use anyhow::Result;
use garden_common::HardwareCapabilities;
use std::path::PathBuf;

/// Detect system manufacturer from DMI/SMBIOS.
///
/// Linux: reads `/sys/class/dmi/id/sys_vendor`. Windows: WMI.
#[cfg(target_os = "linux")]
pub(crate) fn detect_system_manufacturer() -> Option<String> {
    std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn detect_system_manufacturer() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance -ClassName Win32_ComputerSystem).Manufacturer",
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !result.is_empty() {
                return Some(result);
            }
        }
    }
    None
}

/// Detect system product name from DMI/SMBIOS.
///
/// Linux: reads `/sys/class/dmi/id/product_name`. Windows: WMI.
#[cfg(target_os = "linux")]
pub(crate) fn detect_system_product() -> Option<String> {
    std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn detect_system_product() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance -ClassName Win32_ComputerSystem).Model",
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !result.is_empty() {
                return Some(result);
            }
        }
    }
    None
}

/// Load cached hardware capabilities from disk.
///
/// Returns None if cache doesn't exist or is invalid.
/// This allows instant startup while background detection runs.
pub async fn load_cached_capabilities() -> Option<HardwareCapabilities> {
    let path = PathBuf::from(garden_common::constants::paths::config_dir()).join("capabilities.json");

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<HardwareCapabilities>(&content) {
            Ok(caps) => {
                tracing::debug!("Loaded capabilities from cache");
                Some(caps)
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to parse capabilities cache");
                None
            }
        },
        Err(_) => {
            tracing::debug!("No capabilities cache found");
            None
        }
    }
}

/// Save hardware capabilities to disk cache.
///
/// Uses atomic write (temp file + rename) for consistency.
pub async fn save_capabilities_cache(capabilities: &HardwareCapabilities) -> Result<()> {
    let dir = PathBuf::from(garden_common::constants::paths::config_dir());
    tokio::fs::create_dir_all(&dir).await?;

    let path = dir.join("capabilities.json");
    let tmp_path = path.with_extension("json.tmp");

    let content = serde_json::to_string_pretty(capabilities)?;
    tokio::fs::write(&tmp_path, content).await?;

    // Atomic rename
    match tokio::fs::rename(&tmp_path, &path).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Windows doesn't allow rename over existing file
            if cfg!(windows) {
                let _ = tokio::fs::remove_file(&path).await;
                tokio::fs::rename(&tmp_path, &path).await?;
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

/// Create a skeleton capabilities object for instant startup.
///
/// Use this when cache doesn't exist. Background detection will update it.
pub fn create_skeleton(stone_name: String) -> HardwareCapabilities {
    use garden_common::{CpuCapabilities, DetectionStatus, HardwareInventory, MemoryCapabilities};

    let hardware = HardwareInventory {
        cpu: CpuCapabilities {
            model: None,
            cores: 0,
            threads: None,
            architecture: std::env::consts::ARCH.to_string(),
            features: None,
        },
        memory: MemoryCapabilities { total_mb: 0 },
        gpus: vec![],
        disk: None,
        swap_mb: None,
        ai_capabilities: None,
        system_manufacturer: None,
        system_product: None,
    };

    HardwareCapabilities {
        stone_id: None, // Set externally after creation
        stone_name,
        hardware,
        runtime: None,
        detection_status: DetectionStatus::Scanning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::DetectionStatus;

    #[tokio::test]
    async fn test_cache_round_trip() {
        let caps = create_skeleton("test-stone".into());

        // Save
        save_capabilities_cache(&caps).await.expect("save failed");

        // Load
        let loaded = load_cached_capabilities().await.expect("should load");

        assert_eq!(loaded.stone_name, "test-stone");
        assert_eq!(loaded.detection_status, DetectionStatus::Scanning);
    }
}
