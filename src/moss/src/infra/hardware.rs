//! Hardware detection and capabilities management
//!
//! Provides composable functions for detecting system hardware,
//! managing capabilities cache, and progressive detection.

use anyhow::Result;
use garden_common::HardwareCapabilities;
use std::path::PathBuf;

/// Detect Docker availability and version
///
/// Returns version string if Docker is running and functional, None otherwise.
/// This differentiates between:
/// - Docker installed but not running (None)
/// - Docker running and functional (Some("24.0.7"))
async fn detect_docker() -> Option<String> {
    use crate::docker::ContainerRuntime;

    // Try to connect to Docker
    let docker = match ContainerRuntime::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = ?e, "Docker not available (connection failed)");
            return None;
        }
    };

    // Verify Docker is actually functional by pinging it
    if !docker.is_healthy().await {
        tracing::debug!("Docker connected but not healthy (ping failed)");
        return None;
    }

    // Get Docker version
    match docker.get_docker_version().await {
        Ok(version) => {
            tracing::info!(version = %version, "Docker is functional");
            Some(version)
        }
        Err(e) => {
            tracing::debug!(error = ?e, "Docker connected but version unavailable");
            None
        }
    }
}

/// Detect system manufacturer from DMI/SMBIOS
///
/// Linux: reads from /sys/class/dmi/id/sys_vendor
/// Windows: uses WMI (stub for now)
#[cfg(target_os = "linux")]
fn detect_system_manufacturer() -> Option<String> {
    std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn detect_system_manufacturer() -> Option<String> {
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

/// Detect system product name from DMI/SMBIOS
///
/// Linux: reads from /sys/class/dmi/id/product_name
/// Windows: uses WMI (stub for now)
#[cfg(target_os = "linux")]
fn detect_system_product() -> Option<String> {
    std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn detect_system_product() -> Option<String> {
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

/// Load cached hardware capabilities from disk
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

/// Save hardware capabilities to disk cache
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

/// Detect hardware capabilities (CPU, memory, GPU, disk)
///
/// This is a progressive detection:
/// 1. Fast: CPU, memory, disk (< 100ms)
/// 2. Slow: GPU detection (may take seconds)
///
/// Call this in a background task to avoid blocking startup.
pub async fn detect_hardware(stone_name: String) -> Result<HardwareCapabilities> {
    use garden_common::{
        CpuCapabilities, DetectionStatus, DiskCapabilities, HardwareInventory, MemoryCapabilities,
        RuntimeInfo,
    };

    tracing::info!("Starting hardware detection");

    // Fast detection: CPU and memory using resources module
    let (cpu_model, cpu_features, architecture) = garden_common::resources::system::get_cpu_info()
        .unwrap_or_else(|_| {
            (
                "Unknown".to_string(),
                vec![],
                std::env::consts::ARCH.to_string(),
            )
        });

    let resources = garden_common::resources::system::collect_stone_resources().ok();
    let cpu_cores = resources.as_ref().map(|r| r.cpu.cores).unwrap_or(1);
    let total_memory_mb = resources
        .as_ref()
        .map(|r| r.memory.total_bytes / 1024 / 1024)
        .unwrap_or(0);

    let disk = resources.as_ref().map(|r| {
        // Single source: the data partition (offering data + container images live there).
        let primary = r.data_partition();
        DiskCapabilities {
            total_gb: primary.map(|s| s.total_gb).unwrap_or(0),
            disk_type: primary.map(|s| match &s.disk_type {
                garden_common::DiskType::NVMe => "NVMe".to_string(),
                garden_common::DiskType::SSD => "SSD".to_string(),
                garden_common::DiskType::HDD => "HDD".to_string(),
                garden_common::DiskType::Unknown => "Unknown".to_string(),
            }),
        }
    });

    // Slow detection: GPUs
    tracing::debug!("Detecting GPUs (may take a few seconds)...");
    let gpus = garden_common::resources::system::detect_gpus();

    // Additional system info
    let os_version = garden_common::resources::system::detect_os_version();
    let kernel_version = garden_common::resources::system::detect_kernel_version();
    let swap_mb = garden_common::resources::system::detect_swap();

    // DMI/SMBIOS system identity (for hw manifest matching)
    let system_manufacturer = detect_system_manufacturer();
    let system_product = detect_system_product();

    if let (Some(mfr), Some(prod)) = (&system_manufacturer, &system_product) {
        tracing::info!(manufacturer = %mfr, product = %prod, "Detected system identity");
    }

    let hardware = HardwareInventory {
        cpu: CpuCapabilities {
            model: if cpu_model == "Unknown" {
                None
            } else {
                Some(cpu_model.clone())
            },
            cores: cpu_cores,
            threads: None,
            architecture,
            features: if cpu_features.is_empty() {
                None
            } else {
                Some(cpu_features)
            },
        },
        memory: MemoryCapabilities {
            total_mb: total_memory_mb,
        },
        gpus,
        disk,
        swap_mb,
        ai_capabilities: None,
        system_manufacturer,
        system_product,
    };

    // Build OS version string for RuntimeInfo.os
    let os_family = std::env::consts::OS;
    let os_string = if let Some(ref ver) = os_version {
        format!("{}/{}", os_family, ver)
    } else {
        os_family.to_string()
    };

    // Detect Docker (async check for functional Docker daemon)
    let docker_version = detect_docker().await;

    let capabilities = HardwareCapabilities {
        stone_id: None, // Set externally after detection
        stone_name,
        hardware,
        runtime: Some(RuntimeInfo {
            docker_version,
            os: os_string,
            kernel: kernel_version,
        }),
        detection_status: DetectionStatus::Complete,
    };

    tracing::info!(
        cpu = ?capabilities.hardware.cpu.model,
        memory_gb = capabilities.hardware.memory.total_mb / 1024,
        gpus = capabilities.hardware.gpus.len(),
        "Hardware detection complete"
    );

    Ok(capabilities)
}

/// Create a skeleton capabilities object for instant startup
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
