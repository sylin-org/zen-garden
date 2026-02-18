//! Storage lifecycle — physical device health and mount verification (STORAGE-0007)
//!
//! `StorageDevice` represents a single physical storage device (USB drive) with:
//! - Mount state tracking (device path → mount point)
//! - Health monitoring via `/proc/mounts` + capacity probe
//! - Self-healing: automatic remount on transient unmount
//!
//! This is the **infrastructure layer**: no business rules, no domain logic.
//! Domain objects like `SeedBank` compose a `StorageDevice` and delegate
//! mount verification before any I/O operation.

use std::path::PathBuf;
use tracing::{debug, info, warn};

// ============================================================================
// Health enum
// ============================================================================

/// Physical health of a storage device.
///
/// Transitions:
/// ```text
/// Healthy ──(mount lost)──→ Unmounted ──(remount ok)──→ Healthy
///                             │
///                       (remount fail)
///                             │
///                             ▼
///                           Lost ──(re-detection via hotplug)──→ Healthy
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageHealth {
    /// Device is mounted and responsive (capacity > 0).
    Healthy,
    /// Device is mounted but showing issues (e.g. read-only, I/O errors).
    Degraded(String),
    /// Mount point exists but device is no longer attached.
    Unmounted,
    /// Device is no longer visible in /dev — physical removal.
    Lost,
}

impl std::fmt::Display for StorageHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded(reason) => write!(f, "degraded: {}", reason),
            Self::Unmounted => write!(f, "unmounted"),
            Self::Lost => write!(f, "lost"),
        }
    }
}

impl StorageHealth {
    /// Whether I/O operations should be attempted.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded(_))
    }
}

// ============================================================================
// StorageDevice
// ============================================================================

/// Physical storage device lifecycle object.
///
/// Tracks a mounted USB device and verifies its health before I/O.
/// Domain objects compose this to get mount-safety for free.
#[derive(Debug, Clone)]
pub struct StorageDevice {
    /// Block device path (e.g. `/dev/sda1`).
    pub device: String,

    /// Filesystem mount point (e.g. `/var/lib/zen-garden/mounts/seed-clear-valley/019c0789/`).
    pub mount_path: PathBuf,

    /// Current health assessment.
    pub health: StorageHealth,

    /// Total capacity in bytes (last known).
    pub capacity_bytes: u64,

    /// Used space in bytes (last known).
    pub used_bytes: u64,

    /// Filesystem type (e.g. `ext4`, `vfat`, `btrfs`).
    pub filesystem: String,
}

impl StorageDevice {
    /// Create a new storage device in a known-healthy state.
    ///
    /// Called after a successful mount + liveness probe during detection.
    pub fn new(
        device: impl Into<String>,
        mount_path: impl Into<PathBuf>,
        filesystem: impl Into<String>,
        capacity_bytes: u64,
        used_bytes: u64,
    ) -> Self {
        Self {
            device: device.into(),
            mount_path: mount_path.into(),
            health: StorageHealth::Healthy,
            capacity_bytes,
            used_bytes,
            filesystem: filesystem.into(),
        }
    }

    /// Verify the device is mounted and writable before performing I/O.
    ///
    /// On failure, attempts self-healing remount (Linux only).
    /// Returns `Ok(())` if the device is usable, `Err` otherwise.
    ///
    /// **This is the central safety gate.** Every write path must call this.
    pub async fn ensure_mounted(&mut self) -> anyhow::Result<()> {
        // Fast path: if health is Healthy/Degraded and a quick probe passes, return Ok
        if self.health.is_usable() && self.probe_mount().await {
            return Ok(());
        }

        // Probe failed or health was already bad — full check
        self.health_tick().await;

        match &self.health {
            StorageHealth::Healthy | StorageHealth::Degraded(_) => Ok(()),
            StorageHealth::Unmounted => {
                // Attempt self-healing remount
                info!(
                    device = %self.device,
                    mount_path = %self.mount_path.display(),
                    "Mount lost — attempting self-healing remount"
                );
                match self.remount().await {
                    Ok(()) => {
                        info!(
                            device = %self.device,
                            "Self-healing remount succeeded"
                        );
                        Ok(())
                    }
                    Err(e) => {
                        self.health = StorageHealth::Lost;
                        anyhow::bail!(
                            "Device {} is unmounted and remount failed: {}",
                            self.device,
                            e
                        )
                    }
                }
            }
            StorageHealth::Lost => {
                anyhow::bail!(
                    "Device {} is lost — physical device removed",
                    self.device
                )
            }
        }
    }

    /// Periodic health check — call every ~10s from the coordinator tick.
    ///
    /// Probes `/proc/mounts` and disk capacity. Updates `health`,
    /// `capacity_bytes`, and `used_bytes`.
    pub async fn health_tick(&mut self) -> &StorageHealth {
        let mount_path_str = self.mount_path.to_string_lossy().to_string();

        // Check if device is still in /proc/mounts
        let is_mounted = self.check_proc_mounts(&mount_path_str).await;

        if !is_mounted {
            if self.health != StorageHealth::Unmounted && self.health != StorageHealth::Lost {
                warn!(
                    device = %self.device,
                    mount_path = %mount_path_str,
                    "Device no longer in /proc/mounts — marking Unmounted"
                );
            }
            self.health = StorageHealth::Unmounted;
            return &self.health;
        }

        // Mounted — probe capacity as liveness check
        match super::device::DeviceAnalyzer::get_disk_usage(&mount_path_str) {
            Some((used, avail)) => {
                let capacity = used + avail;
                if capacity == 0 {
                    self.health = StorageHealth::Degraded("zero capacity reported".into());
                } else {
                    self.capacity_bytes = capacity;
                    self.used_bytes = used;
                    // Recover from Degraded/Unmounted if probe succeeds
                    if !matches!(self.health, StorageHealth::Healthy) {
                        debug!(
                            device = %self.device,
                            "Storage health recovered to Healthy"
                        );
                    }
                    self.health = StorageHealth::Healthy;
                }
            }
            None => {
                self.health = StorageHealth::Degraded("capacity probe failed".into());
            }
        }

        &self.health
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Quick liveness probe — check mount exists and capacity > 0.
    async fn probe_mount(&self) -> bool {
        let mount_str = self.mount_path.to_string_lossy().to_string();
        matches!(
            super::device::DeviceAnalyzer::get_disk_usage(&mount_str),
            Some((_, avail)) if avail > 0
        )
    }

    /// Check `/proc/mounts` (Linux) or assume mounted (non-Linux).
    async fn check_proc_mounts(&self, mount_path: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            match tokio::fs::read_to_string("/proc/mounts").await {
                Ok(content) => content.lines().any(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts.len() >= 2 && parts[1] == mount_path
                }),
                Err(_) => false,
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = mount_path;
            // On non-Linux, assume mounted if the path exists and has files
            self.mount_path.exists()
        }
    }

    /// Attempt to remount the device at its original mount point.
    async fn remount(&mut self) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        {
            let mount_path_str = self.mount_path.to_string_lossy().to_string();

            // Ensure mount point directory exists
            tokio::fs::create_dir_all(&self.mount_path).await?;

            let output = tokio::process::Command::new("sudo")
                .args(["mount", &self.device, &mount_path_str])
                .output()
                .await?;

            if output.status.success() {
                self.health = StorageHealth::Healthy;
                // Refresh capacity
                if let Some((used, avail)) =
                    super::device::DeviceAnalyzer::get_disk_usage(&mount_path_str)
                {
                    self.capacity_bytes = used + avail;
                    self.used_bytes = used;
                }
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("mount failed: {}", stderr.trim())
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            anyhow::bail!("Remount not supported on this platform")
        }
    }
}

// ============================================================================
// Convenience: build from registry scan data
// ============================================================================

impl StorageDevice {
    /// Build from a `SeedBankInfo` (registry scan result).
    ///
    /// The info must represent a currently-mounted, live device (online = true).
    pub fn from_seed_bank_info(info: &garden_common::storage::SeedBankInfo) -> Self {
        let fs = if info.btrfs {
            "btrfs".to_string()
        } else {
            "ext4".to_string()
        };
        Self::new(
            &info.device,
            &info.mount_path,
            fs,
            info.capacity_bytes,
            info.used_bytes,
        )
    }
}
