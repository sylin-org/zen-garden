//! Cross-platform volume adapter (STORAGE-0011)
//!
//! Thin OS-specific layer that answers two questions:
//! - "What volumes are accessible right now?" → [`scan_volumes()`]
//! - "What changed?" → [`start_volume_watcher()`]
//!
//! Plus utility functions for health probing:
//! - [`disk_usage()`] — (used, available) in bytes
//!
//! Adapters never check manifests, never classify managed vs unmanaged,
//! never emit domain events. They report what the OS sees.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
#[cfg(target_os = "windows")]
use tracing::info;

// ============================================================================
// Types (platform-agnostic)
// ============================================================================

/// Snapshot of a volume as seen by the OS.
///
/// No domain concepts — just what the platform reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSnapshot {
    /// Device identifier: `/dev/sdb1` on Linux, `E:\` on Windows.
    pub path: String,

    /// Where the volume's content is accessible.
    /// On Linux this is the mount point (e.g. `/mnt/usb`).
    /// On Windows this equals `path` (drive letter IS the mount).
    pub mount_path: String,

    /// Filesystem label if available (e.g. "SANDISK_32GB").
    pub label: Option<String>,

    /// Total capacity in bytes.
    pub capacity_bytes: u64,

    /// Whether the OS considers this removable (USB, SD card, etc.).
    pub removable: bool,
}

/// Volume event emitted by the OS adapter.
#[derive(Debug, Clone)]
pub enum VolumeEvent {
    /// A volume became accessible (plugged in, mounted, drive letter assigned).
    Appeared(VolumeSnapshot),
    /// A volume is no longer accessible (unplugged, unmounted).
    Disappeared { path: String },
}

/// Disk usage result.
#[derive(Debug, Clone, Copy)]
pub struct DiskUsage {
    pub used_bytes: u64,
    pub available_bytes: u64,
}

impl DiskUsage {
    pub fn total(&self) -> u64 {
        self.used_bytes + self.available_bytes
    }
}

// ============================================================================
// Medium types — physical disk layer (host-only)
// ============================================================================

/// Bus type of the physical connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BusType {
    Usb,
    Sata,
    Nvme,
    Scsi,
    /// SD card, eMMC
    Mmc,
    Unknown,
}

impl std::fmt::Display for BusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb => write!(f, "USB"),
            Self::Sata => write!(f, "SATA"),
            Self::Nvme => write!(f, "NVMe"),
            Self::Scsi => write!(f, "SCSI"),
            Self::Mmc => write!(f, "MMC"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Condition of a physical medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediumCondition {
    /// No partition table. Brand new or wiped disk.
    Raw,
    /// Has a partition table with 1+ partitions.
    Partitioned,
    /// Device is offline or reporting I/O errors.
    Unreadable,
}

impl std::fmt::Display for MediumCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Partitioned => write!(f, "partitioned"),
            Self::Unreadable => write!(f, "unreadable"),
        }
    }
}

// Conversions to shared API types (garden_common::storage)
impl From<BusType> for garden_common::storage::BusType {
    fn from(b: BusType) -> Self {
        match b {
            BusType::Usb => Self::Usb,
            BusType::Sata => Self::Sata,
            BusType::Nvme => Self::Nvme,
            BusType::Scsi => Self::Scsi,
            BusType::Mmc => Self::Mmc,
            BusType::Unknown => Self::Unknown,
        }
    }
}

impl From<MediumCondition> for garden_common::storage::MediumCondition {
    fn from(c: MediumCondition) -> Self {
        match c {
            MediumCondition::Raw => Self::Raw,
            MediumCondition::Partitioned => Self::Partitioned,
            MediumCondition::Unreadable => Self::Unreadable,
        }
    }
}

/// A partition on a physical medium.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSnapshot {
    /// Partition number (1-based).
    pub index: u32,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Filesystem type if known (e.g., "NTFS", "ext4").
    pub filesystem: Option<String>,
    /// Drive letter (Windows) or mount point (Linux), if assigned.
    pub mount_path: Option<String>,
    /// Volume label if available.
    pub label: Option<String>,
}

/// Snapshot of a physical storage medium (disk) as seen by the OS.
///
/// Represents the whole disk, not a partition. A medium can have 0 or more
/// partitions. Host-only — never broadcast to the garden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediumSnapshot {
    /// OS-level device identifier.
    /// Windows: `\\.\PhysicalDrive2`, Linux: `/dev/sdb`
    pub device_id: String,

    /// Vendor/model name (e.g., "SanDisk Portable SSD").
    pub model: Option<String>,

    /// Serial number if available.
    pub serial: Option<String>,

    /// Physical bus type.
    pub bus_type: BusType,

    /// Total size in bytes (entire disk).
    pub size_bytes: u64,

    /// Whether the medium is external/removable.
    pub removable: bool,

    /// Physical condition.
    pub condition: MediumCondition,

    /// Partitions on this medium.
    pub partitions: Vec<PartitionSnapshot>,
}

// ============================================================================
// Cross-platform API
// ============================================================================

/// Scan all currently accessible volumes.
///
/// Returns removable and fixed volumes that have mounted filesystems.
/// The domain decides which ones are interesting.
pub fn scan_volumes() -> Vec<VolumeSnapshot> {
    #[cfg(target_os = "linux")]
    {
        linux::scan_volumes()
    }
    #[cfg(target_os = "windows")]
    {
        windows::scan_volumes()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// Get disk usage for a mounted path.
pub fn disk_usage(path: &str) -> Option<DiskUsage> {
    #[cfg(target_os = "linux")]
    {
        linux::disk_usage(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::disk_usage(path)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        None
    }
}

/// Start a volume watcher that emits events on a channel.
///
/// Runs until the channel is closed. Platform-specific implementation:
/// - Linux: udev monitor (blocking thread) + periodic scan fallback
/// - Windows: polling drive letters every 5 seconds
pub fn start_volume_watcher(tx: tokio::sync::mpsc::Sender<VolumeEvent>) {
    #[cfg(target_os = "linux")]
    {
        linux::start_volume_watcher(tx);
    }
    #[cfg(target_os = "windows")]
    {
        windows::start_volume_watcher(tx);
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = tx;
        warn!("Volume watcher not supported on this platform");
    }
}

/// Scan physical storage media (disks).
///
/// Returns every physical disk the OS can see, including those without
/// partition tables or drive letters. The domain uses this for candidate
/// discovery — "what physical devices are available and in what condition?"
///
/// This is heavier than `scan_volumes()` (spawns PowerShell / lsblk) and
/// should be called at a lower cadence (10–30 s, not 5 s).
pub fn scan_media() -> Vec<MediumSnapshot> {
    #[cfg(target_os = "linux")]
    {
        linux::scan_media()
    }
    #[cfg(target_os = "windows")]
    {
        windows::scan_media()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// Mount a block device at the given path.
///
/// Linux: `sudo mount <device> <mount_path>` with timeout.
/// Windows: Not supported (volumes are auto-assigned drive letters by the OS).
pub async fn mount_device(device: &str, mount_path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::mount_device(device, mount_path).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device, mount_path);
        anyhow::bail!("mount_device not supported on this platform")
    }
}

/// Unmount a filesystem at the given mount path.
///
/// Linux: `sudo umount <mount_path>` with timeout.
/// Windows: Not supported.
pub async fn unmount(mount_path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::unmount(mount_path).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_path;
        anyhow::bail!("unmount not supported on this platform")
    }
}

/// Lazy unmount — detach the filesystem immediately, clean up references later.
///
/// Linux: `sudo umount -l <mount_path>` with timeout.
/// Useful for NAS mounts that may hang on a dead server.
pub async fn unmount_lazy(mount_path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::unmount_lazy(mount_path).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_path;
        anyhow::bail!("unmount_lazy not supported on this platform")
    }
}

/// Temp-mount a device read-only, read `.zen-garden/manifest.json`, unmount.
///
/// Returns `None` if the device has no manifest (not a managed storage).
/// The temp mount point is cleaned up even on error.
pub async fn probe_device_manifest(
    device: &str,
) -> anyhow::Result<Option<garden_common::storage::StorageManifest>> {
    #[cfg(target_os = "linux")]
    {
        linux::probe_device_manifest(device).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device;
        Ok(None)
    }
}

/// Check whether a specific block device is currently mounted.
///
/// Linux: reads `/proc/mounts`. Windows: always false.
pub fn is_device_mounted(device: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::is_device_mounted(device)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device;
        false
    }
}

/// Check whether a path is a mount point (has a filesystem mounted on it).
///
/// Linux: reads `/proc/mounts`. Windows: always false.
pub fn is_mount_point(path: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::is_mount_point(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// Return the device currently mounted at `mount_path`, if any.
///
/// Linux: reads `/proc/mounts`. Windows: returns None.
pub fn device_at_mount_point(mount_path: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux::device_at_mount_point(mount_path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_path;
        None
    }
}

/// Get the mount point for a device, if it's currently mounted.
///
/// Linux: reads `/proc/mounts`. Windows: returns None.
pub fn mount_point_for_device(device: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux::mount_point_for_device(device)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device;
        None
    }
}

/// Get the capacity of a block device in bytes.
///
/// Linux: reads `/sys/class/block/{dev}/size`. Other platforms: returns 0.
pub fn device_capacity(device_path: &str) -> u64 {
    #[cfg(target_os = "linux")]
    {
        linux::capacity_from_sysfs(device_path).unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device_path;
        0
    }
}

/// Get the filesystem label of a block device.
///
/// Linux: runs `lsblk -no LABEL`. Other platforms: returns None.
pub fn device_label(device_path: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux::label_from_lsblk(device_path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device_path;
        None
    }
}

/// Probe a block device to determine its state (filesystem type, contents).
///
/// Linux: uses `blkid` to detect filesystem, optionally temp-mounts to inspect
/// contents, validates `.zen-garden/` manifests.
/// Other platforms: returns `HasData` (cannot probe block devices).
pub fn probe_device_state(
    device_path: &str,
    mount_path: Option<&str>,
) -> anyhow::Result<garden_common::storage::DeviceState> {
    #[cfg(target_os = "linux")]
    {
        linux::probe_device_state(device_path, mount_path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_path, mount_path);
        Ok(garden_common::storage::DeviceState::HasData)
    }
}

/// An unmounted removable device that could potentially be a managed storage.
#[derive(Debug, Clone)]
pub struct UnmountedDevice {
    /// Device path (e.g., `/dev/sdb1`).
    pub device: String,
    /// Device name (e.g., `sdb1`).
    pub name: String,
    /// Capacity in bytes (from sysfs).
    pub capacity_bytes: u64,
    /// Filesystem label if available.
    pub label: Option<String>,
}

/// List unmounted removable devices that could potentially be managed storage.
///
/// Linux: scans `/sys/block` for removable devices with unmounted partitions.
/// Windows: returns empty (Windows auto-assigns drive letters).
pub fn list_unmounted_removable() -> Vec<UnmountedDevice> {
    #[cfg(target_os = "linux")]
    {
        linux::list_unmounted_removable()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// Check whether a path is on a removable device.
pub fn is_removable(path: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::is_removable(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::is_removable(path)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        false
    }
}

// ============================================================================
// Linux implementation
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::path::Path;

    /// Scan all mounted volumes via /proc/mounts + sysfs.
    pub fn scan_volumes() -> Vec<VolumeSnapshot> {
        let mut results = Vec::new();

        let mounts = match std::fs::read_to_string("/proc/mounts") {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "Failed to read /proc/mounts");
                return results;
            }
        };

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }

            let device = parts[0];
            let mount_path = parts[1];
            let _fs_type = parts[2];

            // Only real block devices
            if !device.starts_with("/dev/") {
                continue;
            }

            // Skip system partitions
            if mount_path == "/"
                || mount_path == "/boot"
                || mount_path.starts_with("/boot/")
                || mount_path == "/home"
                || mount_path.starts_with("/snap/")
            {
                continue;
            }

            // Allowed mount locations for storage
            let dominated = mount_path.starts_with("/mnt/")
                || mount_path.starts_with("/media/")
                || mount_path.starts_with("/run/media/")
                || mount_path.starts_with("/var/lib/zen-garden/mounts/");

            if !dominated {
                continue;
            }

            let removable = is_removable(device);
            let capacity = capacity_from_sysfs(device).unwrap_or(0);
            let label = label_from_lsblk(device);

            results.push(VolumeSnapshot {
                path: device.to_string(),
                mount_path: mount_path.to_string(),
                label,
                capacity_bytes: capacity,
                removable,
            });
        }

        debug!(count = results.len(), "Linux volume scan complete");
        results
    }

    pub fn is_removable(device_path: &str) -> bool {
        let base_name = base_device_name(device_path);

        // Method 1: sysfs removable flag
        let removable_path = format!("/sys/block/{}/removable", base_name);
        if let Ok(content) = std::fs::read_to_string(&removable_path) {
            if content.trim() == "1" {
                return true;
            }
        }

        // Method 2: USB bus via canonical path
        let device_sysfs = format!("/sys/block/{}/device", base_name);
        if let Ok(canonical) = std::fs::canonicalize(&device_sysfs) {
            let path_str = canonical.to_string_lossy();
            if path_str.contains("/usb") || path_str.contains("/mmc") {
                return true;
            }
        }

        // Method 3: uevent driver
        let uevent_path = format!("/sys/block/{}/device/uevent", base_name);
        if let Ok(content) = std::fs::read_to_string(&uevent_path) {
            if content.contains("DRIVER=usb-storage") || content.contains("DRIVER=uas") {
                return true;
            }
        }

        // Method 4: sysfs link
        let device_path_sysfs = format!("/sys/block/{}", base_name);
        if let Ok(link) = std::fs::read_link(&device_path_sysfs) {
            let link_str = link.to_string_lossy();
            if link_str.contains("/usb") || link_str.contains("usb-storage") {
                return true;
            }
        }

        // Method 5: lsblk TRAN field
        if let Ok(output) = super::super::subprocess::run_command_timed_sync(
            "lsblk",
            &["-dno", "TRAN", device_path],
            std::time::Duration::from_secs(5),
        ) {
            if output.status.success() {
                let tran = String::from_utf8_lossy(&output.stdout);
                let tran = tran.trim().to_lowercase();
                if tran == "usb" || tran == "mmc" || tran == "sas" {
                    return true;
                }
            }
        }

        false
    }

    pub fn disk_usage(path: &str) -> Option<DiskUsage> {
        let output = super::super::subprocess::run_command_timed_sync(
            "df",
            &["-B1", "--output=used,avail", path],
            std::time::Duration::from_secs(5),
        )
        .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 2 {
            return None;
        }
        let parts: Vec<&str> = lines[1].split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let used: u64 = parts[0].parse().ok()?;
        let avail: u64 = parts[1].parse().ok()?;
        Some(DiskUsage {
            used_bytes: used,
            available_bytes: avail,
        })
    }

    pub fn start_volume_watcher(tx: tokio::sync::mpsc::Sender<VolumeEvent>) {
        // Try udev first, fall back to polling
        let udev_tx = tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = run_udev_watcher(udev_tx) {
                warn!(error = %e, "udev watcher failed, falling back to polling");
            }
        });

        // Polling fallback (also catches mount changes udev misses)
        tokio::spawn(async move {
            let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Initialize with current state
            for v in scan_volumes() {
                known.insert(v.path.clone());
            }

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

                let current = scan_volumes();
                let current_paths: std::collections::HashSet<String> =
                    current.iter().map(|v| v.path.clone()).collect();

                // New volumes
                for v in &current {
                    if !known.contains(&v.path) {
                        debug!(path = %v.path, "Volume appeared (polling)");
                        if tx.send(VolumeEvent::Appeared(v.clone())).await.is_err() {
                            return; // channel closed
                        }
                    }
                }

                // Departed volumes
                for path in &known {
                    if !current_paths.contains(path) {
                        debug!(path = %path, "Volume disappeared (polling)");
                        if tx
                            .send(VolumeEvent::Disappeared {
                                path: path.clone(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }

                known = current_paths;
            }
        });
    }

    fn run_udev_watcher(tx: tokio::sync::mpsc::Sender<VolumeEvent>) -> anyhow::Result<()> {
        use std::os::unix::io::AsRawFd;

        let socket = udev::MonitorBuilder::new()
            .context("Failed to create udev monitor")?
            .match_subsystem("block")
            .context("Failed to set subsystem filter")?
            .listen()
            .context("Failed to start udev monitor")?;

        tracing::info!("udev volume watcher started");

        loop {
            let mut pollfd = libc::pollfd {
                fd: socket.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };

            let ret = unsafe { libc::poll(&mut pollfd, 1, 5000) };
            if ret < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err.into());
            }
            if ret == 0 {
                continue;
            }

            while let Some(event) = socket.iter().next() {
                let devnode = match event.devnode() {
                    Some(node) => node.to_string_lossy().to_string(),
                    None => continue,
                };

                match event.event_type() {
                    udev::EventType::Add => {
                        debug!(device = %devnode, "udev: block device added");
                        // Wait briefly for the device to settle (mount may not be instant)
                        std::thread::sleep(std::time::Duration::from_millis(500));

                        // The polling loop will pick up the mounted volume.
                        // We can also try to build a snapshot directly if it's already mounted.
                        if let Some(snapshot) = build_snapshot_for_device(&devnode) {
                            let _ = tx.blocking_send(VolumeEvent::Appeared(snapshot));
                        }
                    }
                    udev::EventType::Remove => {
                        debug!(device = %devnode, "udev: block device removed");
                        let _ = tx.blocking_send(VolumeEvent::Disappeared { path: devnode });
                    }
                    _ => {}
                }
            }
        }
    }

    fn build_snapshot_for_device(device: &str) -> Option<VolumeSnapshot> {
        // Check if this device is mounted
        let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == device {
                let mount_path = parts[1];
                return Some(VolumeSnapshot {
                    path: device.to_string(),
                    mount_path: mount_path.to_string(),
                    label: label_from_lsblk(device),
                    capacity_bytes: capacity_from_sysfs(device).unwrap_or(0),
                    removable: is_removable(device),
                });
            }
        }
        None
    }

    fn base_device_name(device_path: &str) -> String {
        let name = Path::new(device_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        name.chars()
            .take_while(|c| !c.is_ascii_digit())
            .collect()
    }

    pub(super) fn capacity_from_sysfs(device_path: &str) -> Option<u64> {
        let device_name = Path::new(device_path)
            .file_name()
            .and_then(|n| n.to_str())?;
        let size_path = format!("/sys/class/block/{}/size", device_name);
        let content = std::fs::read_to_string(size_path).ok()?;
        let sectors: u64 = content.trim().parse().ok()?;
        Some(sectors * 512)
    }

    pub(super) fn label_from_lsblk(device_path: &str) -> Option<String> {
        let output = super::super::subprocess::run_command_timed_sync(
            "lsblk",
            &["-no", "LABEL", device_path],
            std::time::Duration::from_secs(5),
        )
        .ok()?;

        if output.status.success() {
            let label = String::from_utf8_lossy(&output.stdout);
            let label = label.trim();
            if !label.is_empty() {
                return Some(label.to_string());
            }
        }
        None
    }

    use anyhow::Context;

    // ========================================================================
    // Device state probing
    // ========================================================================

    /// Probe a block device to determine its state.
    ///
    /// Runs `blkid` to detect filesystem presence. If a filesystem is found and
    /// the device is mounted, inspects the mount contents. If not mounted,
    /// temp-mounts read-only to inspect, then cleans up.
    pub fn probe_device_state(
        device_path: &str,
        mount_path: Option<&str>,
    ) -> anyhow::Result<garden_common::storage::DeviceState> {
        use super::super::subprocess::run_command_timed_sync;
        use garden_common::constants::timeouts;
        use garden_common::storage::DeviceState;

        let query_timeout = timeouts::subprocess_query_timeout();
        let mount_timeout = timeouts::subprocess_mount_timeout();

        let output = run_command_timed_sync(
            "blkid",
            &["-o", "value", "-s", "TYPE", device_path],
            query_timeout,
        )
        .context("Failed to run blkid")?;

        if !output.status.success() || output.stdout.is_empty() {
            let output = run_command_timed_sync("blkid", &["-p", device_path], query_timeout)
                .context("Failed to run blkid -p")?;

            if output.stdout.is_empty() {
                return Ok(DeviceState::Unpartitioned);
            }
            return Ok(DeviceState::Unformatted);
        }

        // Has filesystem — check contents if mounted
        if let Some(mount) = mount_path {
            return check_mount_contents(mount);
        }

        // Not mounted — try to mount temporarily to inspect
        let temp_mount = format!("/tmp/zen-garden-inspect-{}", std::process::id());

        let _ = run_command_timed_sync("sudo", &["mkdir", "-p", &temp_mount], mount_timeout);

        if Path::new(&temp_mount).exists() {
            let mount_result = run_command_timed_sync(
                "sudo",
                &["mount", "-o", "ro", device_path, &temp_mount],
                mount_timeout,
            );

            if let Ok(output) = mount_result {
                if output.status.success() {
                    let result = check_mount_contents(&temp_mount);

                    let _ =
                        run_command_timed_sync("sudo", &["umount", &temp_mount], mount_timeout);
                    let _ =
                        run_command_timed_sync("sudo", &["rmdir", &temp_mount], mount_timeout);

                    return result;
                }
            }
            let _ = run_command_timed_sync("sudo", &["rmdir", &temp_mount], mount_timeout);
        }

        Ok(DeviceState::Unformatted)
    }

    /// Check contents of a mounted filesystem to classify device state.
    fn check_mount_contents(
        mount_path: &str,
    ) -> anyhow::Result<garden_common::storage::DeviceState> {
        use garden_common::storage::DeviceState;

        let mount_dir = Path::new(mount_path);

        let zen_dir = mount_dir.join(".zen-garden");
        if zen_dir.exists() {
            if crate::domain::storage::validate_manifest(&zen_dir).is_ok() {
                return Ok(DeviceState::Prepared);
            } else {
                tracing::debug!("Corrupt or incomplete manifest at {:?}", zen_dir);
                return Ok(DeviceState::HasData);
            }
        }

        let has_visible_files = std::fs::read_dir(mount_dir)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy();
                    !name_str.starts_with('.')
                        && name_str != "System Volume Information"
                        && name_str != "$RECYCLE.BIN"
                })
            })
            .unwrap_or(false);

        if has_visible_files {
            return Ok(DeviceState::HasData);
        }

        Ok(DeviceState::Empty)
    }

    // ========================================================================
    // Mount / unmount / probe operations
    // ========================================================================

    pub async fn mount_device(device: &str, mount_path: &str) -> anyhow::Result<()> {
        use super::super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        tokio::fs::create_dir_all(mount_path).await?;

        let output =
            run_sudo_timed_quiet(&["mount", device, mount_path], timeouts::subprocess_mount_timeout())
                .await
                .context("mount command failed or timed out")?;

        if output.status.success() {
            tracing::info!(device = %device, mount = %mount_path, "Mounted device");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("mount {} -> {} failed: {}", device, mount_path, stderr.trim())
        }
    }

    pub async fn unmount(mount_path: &str) -> anyhow::Result<()> {
        use super::super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let output =
            run_sudo_timed_quiet(&["umount", mount_path], timeouts::subprocess_mount_timeout())
                .await
                .context("umount command failed or timed out")?;

        if output.status.success() {
            tracing::info!(mount = %mount_path, "Unmounted");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("umount {} failed: {}", mount_path, stderr.trim())
        }
    }

    pub async fn unmount_lazy(mount_path: &str) -> anyhow::Result<()> {
        use super::super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let output = run_sudo_timed_quiet(
            &["umount", "-l", mount_path],
            timeouts::subprocess_mount_timeout(),
        )
        .await
        .context("umount -l command failed or timed out")?;

        if output.status.success() {
            tracing::info!(mount = %mount_path, "Lazy-unmounted");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("umount -l {} failed: {}", mount_path, stderr.trim())
        }
    }

    pub async fn probe_device_manifest(
        device: &str,
    ) -> anyhow::Result<Option<garden_common::storage::StorageManifest>> {
        use super::super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let mount_timeout = timeouts::subprocess_mount_timeout();
        let temp_mount = format!(
            "/tmp/zen-garden-probe-{}-{}",
            std::process::id(),
            device.replace('/', "_")
        );

        // Create temp mount point
        let _ = run_sudo_timed_quiet(&["mkdir", "-p", &temp_mount], mount_timeout).await;

        // Try to mount read-only
        let mount_result =
            run_sudo_timed_quiet(&["mount", "-o", "ro", device, &temp_mount], mount_timeout).await;

        let manifest = if let Ok(output) = mount_result {
            if output.status.success() {
                let manifest_path = format!("{}/.zen-garden/manifest.json", temp_mount);
                let manifest = if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await
                {
                    match serde_json::from_str::<garden_common::storage::StorageManifest>(&content) {
                        Ok(m) => {
                            debug!(device = %device, name = %m.name, id = %m.id, "Probed manifest");
                            Some(m)
                        }
                        Err(e) => {
                            warn!(device = %device, error = %e, "Found manifest but failed to parse");
                            None
                        }
                    }
                } else {
                    None
                };

                // Unmount
                let _ = run_sudo_timed_quiet(&["umount", &temp_mount], mount_timeout).await;
                manifest
            } else {
                None
            }
        } else {
            None
        };

        // Cleanup temp mount point
        let _ = run_sudo_timed_quiet(&["rmdir", &temp_mount], mount_timeout).await;
        Ok(manifest)
    }

    pub fn is_device_mounted(device: &str) -> bool {
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() && parts[0] == device {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_mount_point(path: &str) -> bool {
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == path {
                    return true;
                }
            }
        }
        false
    }

    /// Return the device currently mounted at `mount_path`, or `None`.
    pub fn device_at_mount_point(mount_path: &str) -> Option<String> {
        let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == mount_path {
                return Some(parts[0].to_string());
            }
        }
        None
    }

    pub fn mount_point_for_device(device: &str) -> Option<String> {
        let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() && parts[0] == device && parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }
        None
    }

    pub fn list_unmounted_removable() -> Vec<super::UnmountedDevice> {
        use super::super::subprocess::run_command_timed_sync;

        let mut results = Vec::new();

        // Get all currently mounted devices
        let mounted_devices: std::collections::HashSet<String> =
            std::fs::read_to_string("/proc/mounts")
                .map(|content| {
                    content
                        .lines()
                        .filter_map(|line| line.split_whitespace().next())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

        let sys_block = Path::new("/sys/block");
        let entries = match std::fs::read_dir(sys_block) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to read /sys/block");
                return results;
            }
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let device_name = entry.file_name();
            let device_name_str = device_name.to_string_lossy();

            // Only real disk devices
            if !device_name_str.starts_with("sd") && !device_name_str.starts_with("nvme") {
                continue;
            }

            let device_path = format!("/dev/{}", device_name_str);

            if !is_removable(&device_path) {
                continue;
            }

            // Find partitions
            let device_sys_path = entry.path();
            let mut found_partitions = false;
            if let Ok(contents) = std::fs::read_dir(&device_sys_path) {
                for part_entry in contents.filter_map(|e| e.ok()) {
                    let part_name = part_entry.file_name();
                    let part_name_str = part_name.to_string_lossy();

                    if part_name_str.starts_with(&*device_name_str)
                        && part_name_str != device_name_str
                    {
                        found_partitions = true;

                        let part_path = format!("/dev/{}", part_name_str);

                        if mounted_devices.contains(&part_path) {
                            continue;
                        }

                        // Check if it has a filesystem
                        let has_fs = run_command_timed_sync(
                            "blkid",
                            &["-o", "value", "-s", "TYPE", &part_path],
                            std::time::Duration::from_secs(5),
                        )
                        .map(|o| o.status.success() && !o.stdout.is_empty())
                        .unwrap_or(false);

                        if !has_fs {
                            continue;
                        }

                        let capacity = capacity_from_sysfs(&part_path).unwrap_or(0);
                        let label = label_from_lsblk(&part_path);

                        results.push(super::UnmountedDevice {
                            device: part_path,
                            name: part_name_str.to_string(),
                            capacity_bytes: capacity,
                            label,
                        });
                    }
                }
            }

            // Check whole-disk only when the device has no partition table
            if !found_partitions && !mounted_devices.contains(&device_path) {
                let has_fs = run_command_timed_sync(
                    "blkid",
                    &["-o", "value", "-s", "TYPE", &device_path],
                    std::time::Duration::from_secs(5),
                )
                .map(|o| o.status.success() && !o.stdout.is_empty())
                .unwrap_or(false);

                if has_fs {
                    let capacity = capacity_from_sysfs(&device_path).unwrap_or(0);
                    let label = label_from_lsblk(&device_path);

                    results.push(super::UnmountedDevice {
                        device: device_path,
                        name: device_name_str.to_string(),
                        capacity_bytes: capacity,
                        label,
                    });
                }
            }
        }

        debug!(count = results.len(), "Unmounted removable devices found");
        results
    }

    /// Scan physical storage media via `lsblk --json`.
    pub fn scan_media() -> Vec<MediumSnapshot> {
        let output = std::process::Command::new("lsblk")
            .args([
                "--json",
                "--bytes",
                "--output",
                "NAME,MODEL,SERIAL,TRAN,SIZE,RM,TYPE,MOUNTPOINT,FSTYPE,LABEL",
            ])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                warn!(stderr = %String::from_utf8_lossy(&o.stderr), "lsblk failed");
                return Vec::new();
            }
            Err(e) => {
                warn!(error = %e, "Failed to spawn lsblk for media scan");
                return Vec::new();
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "Failed to parse lsblk JSON");
                return Vec::new();
            }
        };

        let devices = match parsed.get("blockdevices").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return Vec::new(),
        };

        // Find system disk (contains /)
        let system_disk: Option<&str> = devices.iter().find_map(|dev| {
            let children = dev.get("children")?.as_array()?;
            let has_root = children.iter().any(|c| {
                c.get("mountpoint")
                    .and_then(|v| v.as_str())
                    .map(|m| m == "/")
                    .unwrap_or(false)
            });
            if has_root {
                dev.get("name").and_then(|v| v.as_str())
            } else {
                None
            }
        });

        let mut results = Vec::new();

        for dev in devices {
            let dev_type = dev.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if dev_type != "disk" {
                continue;
            }

            let name = dev.get("name").and_then(|v| v.as_str()).unwrap_or("");

            // Skip system disk
            if Some(name) == system_disk {
                continue;
            }

            // Skip loop, ram, etc.
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
                continue;
            }

            let model = dev.get("model").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
            let serial = dev.get("serial").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
            let tran = dev.get("tran").and_then(|v| v.as_str()).unwrap_or("");
            let size = dev.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            let rm = dev.get("rm").and_then(|v| v.as_bool()).unwrap_or(false);

            let bus_type = match tran {
                "usb" => BusType::Usb,
                "sata" | "ata" => BusType::Sata,
                "nvme" => BusType::Nvme,
                "scsi" | "sas" => BusType::Scsi,
                "mmc" | "sd" => BusType::Mmc,
                _ => BusType::Unknown,
            };

            let removable = rm || bus_type == BusType::Usb || bus_type == BusType::Mmc;

            let children = dev.get("children").and_then(|v| v.as_array());

            let (condition, partitions) = match children {
                None => (MediumCondition::Raw, Vec::new()),
                Some(parts) if parts.is_empty() => {
                    (MediumCondition::Raw, Vec::new())
                }
                Some(parts) => {
                    let parts_vec: Vec<PartitionSnapshot> = parts
                        .iter()
                        .filter(|p| {
                            p.get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "part")
                                .unwrap_or(false)
                        })
                        .enumerate()
                        .map(|(i, p)| PartitionSnapshot {
                            index: (i + 1) as u32,
                            size_bytes: p.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
                            filesystem: p
                                .get("fstype")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                            mount_path: p
                                .get("mountpoint")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                            label: p
                                .get("label")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                        })
                        .collect();
                    (MediumCondition::Partitioned, parts_vec)
                }
            };

            results.push(MediumSnapshot {
                device_id: format!("/dev/{}", name),
                model,
                serial,
                bus_type,
                size_bytes: size,
                removable,
                condition,
                partitions,
            });
        }

        debug!(count = results.len(), "Linux media scan complete");
        results
    }
}

// ============================================================================
// Windows implementation
// ============================================================================

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    // Win32 drive type constants (windows-sys 0.52 doesn't re-export these)
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    pub fn scan_volumes() -> Vec<VolumeSnapshot> {
        use windows_sys::Win32::Storage::FileSystem::{
            GetDriveTypeW, GetLogicalDriveStringsW,
        };

        let mut results = Vec::new();

        // Get all drive letter strings
        let mut buf = [0u16; 256];
        let len = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
        if len == 0 {
            warn!("GetLogicalDriveStringsW failed");
            return results;
        }

        // Parse null-separated drive strings: "C:\\\0D:\\\0\0"
        let mut start = 0;
        for i in 0..(len as usize) {
            if buf[i] == 0 {
                if i > start {
                    let drive: Vec<u16> = buf[start..=i].to_vec();
                    let drive_str = String::from_utf16_lossy(&buf[start..i]);

                    let drive_type = unsafe { GetDriveTypeW(drive.as_ptr()) };

                    // Only removable and fixed drives (skip network, CDROM, etc.)
                    if drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_FIXED {
                        // GetDriveTypeW is unreliable for USB detection — modern
                        // USB SSDs and many flash drives report DRIVE_FIXED.
                        // Check the physical disk bus type via IOCTL as ground truth.
                        let usb_bus = is_usb_bus(&drive_str);
                        let removable = drive_type == DRIVE_REMOVABLE || usb_bus;

                        // Skip C:\ (system drive)
                        if drive_str.eq_ignore_ascii_case("C:\\")
                            || drive_str.eq_ignore_ascii_case("C:/")
                        {
                            start = i + 1;
                            continue;
                        }

                        let label = get_volume_label(&drive);
                        let capacity = get_capacity(&drive_str);

                        results.push(VolumeSnapshot {
                            path: drive_str.clone(),
                            mount_path: drive_str,
                            label,
                            capacity_bytes: capacity,
                            removable,
                        });
                    }
                }
                start = i + 1;
            }
        }

        debug!(count = results.len(), "Windows volume scan complete");
        results
    }

    pub fn is_removable(path: &str) -> bool {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

        let wide = to_wide(path);
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        drive_type == DRIVE_REMOVABLE || is_usb_bus(path)
    }

    pub fn disk_usage(path: &str) -> Option<DiskUsage> {
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let wide = to_wide(path);
        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes_available,
                &mut total_bytes,
                &mut total_free_bytes,
            )
        };

        if ok == 0 {
            return None;
        }

        Some(DiskUsage {
            used_bytes: total_bytes.saturating_sub(total_free_bytes),
            available_bytes: free_bytes_available,
        })
    }

    pub fn start_volume_watcher(tx: tokio::sync::mpsc::Sender<VolumeEvent>) {
        tokio::spawn(async move {
            let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Initialize with current state
            let initial = scan_volumes();
            info!(
                count = initial.len(),
                drives = %initial.iter().map(|v| v.path.as_str()).collect::<Vec<_>>().join(", "),
                "Volume watcher initialized"
            );
            for v in initial {
                known.insert(v.path.clone());
            }

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let current = scan_volumes();
                let current_paths: std::collections::HashSet<String> =
                    current.iter().map(|v| v.path.clone()).collect();

                for v in &current {
                    if !known.contains(&v.path) {
                        info!(
                            path = %v.path,
                            label = ?v.label,
                            removable = v.removable,
                            capacity_gb = v.capacity_bytes / 1_000_000_000,
                            "Volume appeared (watcher)"
                        );
                        if tx.send(VolumeEvent::Appeared(v.clone())).await.is_err() {
                            return;
                        }
                    }
                }

                for path in &known {
                    if !current_paths.contains(path) {
                        info!(path = %path, "Volume disappeared (watcher)");
                        if tx
                            .send(VolumeEvent::Disappeared {
                                path: path.clone(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }

                known = current_paths;
            }
        });
    }

    fn get_volume_label(drive_wide: &[u16]) -> Option<String> {
        use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

        let mut label_buf = [0u16; 256];
        let ok = unsafe {
            GetVolumeInformationW(
                drive_wide.as_ptr(),
                label_buf.as_mut_ptr(),
                label_buf.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };

        if ok == 0 {
            return None;
        }

        let len = label_buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len == 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&label_buf[..len]))
    }

    fn get_capacity(path: &str) -> u64 {
        disk_usage(path)
            .map(|du| du.used_bytes + du.available_bytes)
            .unwrap_or(0)
    }

    /// Check if a drive letter is on a USB bus.
    ///
    /// Opens the volume device (`\\.\X:`) and queries the storage device
    /// descriptor via `IOCTL_STORAGE_QUERY_PROPERTY`. The `BusType` field
    /// in the descriptor tells us whether the physical disk is attached
    /// via USB, regardless of what `GetDriveTypeW` reports.
    ///
    /// Returns `false` on any failure (access denied, not a volume, etc.)
    /// rather than propagating errors — this is a best-effort check.
    fn is_usb_bus(drive_path: &str) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;

        // IOCTL_STORAGE_QUERY_PROPERTY = CTL_CODE(IOCTL_STORAGE_BASE, 0x0500, METHOD_BUFFERED, FILE_ANY_ACCESS)
        const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;
        // StorageDeviceProperty = 0, PropertyStandardQuery = 0
        const STORAGE_DEVICE_PROPERTY: u32 = 0;
        const PROPERTY_STANDARD_QUERY: u32 = 0;
        // BusTypeUsb = 7
        const BUS_TYPE_USB: u32 = 7;

        // Build device path: "\\.\X:" from "X:\" or "X:"
        let letter = drive_path.trim_end_matches('\\').trim_end_matches('/');
        let device_path = format!("\\\\.\\{}", letter);
        let wide = to_wide(&device_path);

        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                0, // No read/write access needed for IOCTL query
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                0,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return false;
        }

        // STORAGE_PROPERTY_QUERY struct: { PropertyId: u32, QueryType: u32, AdditionalParameters: [u8; 1] }
        #[repr(C)]
        struct StoragePropertyQuery {
            property_id: u32,
            query_type: u32,
            additional: [u8; 1],
        }

        let query = StoragePropertyQuery {
            property_id: STORAGE_DEVICE_PROPERTY,
            query_type: PROPERTY_STANDARD_QUERY,
            additional: [0],
        };

        // STORAGE_DEVICE_DESCRIPTOR is variable-length; 256 bytes is plenty
        let mut out_buf = [0u8; 256];
        let mut bytes_returned: u32 = 0;

        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &query as *const _ as *const _,
                std::mem::size_of::<StoragePropertyQuery>() as u32,
                out_buf.as_mut_ptr() as *mut _,
                out_buf.len() as u32,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        unsafe { CloseHandle(handle) };

        if ok == 0 || bytes_returned < 28 {
            return false;
        }

        // STORAGE_DEVICE_DESCRIPTOR layout:
        //   offset 0:  Version (u32)
        //   offset 4:  Size (u32)
        //   offset 8:  DeviceType (u8)
        //   offset 9:  DeviceTypeModifier (u8)
        //   offset 10: RemovableMedia (u8)     ← also useful
        //   offset 11: CommandQueueing (u8)
        //   offset 12: VendorIdOffset (u32)
        //   offset 16: ProductIdOffset (u32)
        //   offset 20: ProductRevisionOffset (u32)
        //   offset 24: SerialNumberOffset (u32)
        //   offset 28: BusType (u32)           ← what we need
        let bus_type = u32::from_le_bytes([out_buf[28], out_buf[29], out_buf[30], out_buf[31]]);

        bus_type == BUS_TYPE_USB
    }

    /// Convert a Rust string to a null-terminated UTF-16 wide string.
    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Scan physical storage media via PowerShell.
    ///
    /// Uses `Get-Disk` and `Get-Partition` to enumerate physical disks
    /// and their partitions — sees drives even without letters or partitions.
    pub fn scan_media() -> Vec<MediumSnapshot> {
        // Single PowerShell invocation that returns disks with nested partitions.
        // ConvertTo-Json -Depth 3 ensures partition arrays are serialized.
        let script = r#"
$disks = Get-Disk | Where-Object { $_.OperationalStatus -ne 'Missing' } | ForEach-Object {
    $d = $_
    $parts = @(Get-Partition -DiskNumber $d.Number -ErrorAction SilentlyContinue | ForEach-Object {
        $vol = Get-Volume -Partition $_ -ErrorAction SilentlyContinue
        [PSCustomObject]@{
            Index      = $_.PartitionNumber
            SizeBytes  = $_.Size
            DriveLetter = if ($_.DriveLetter) { "$($_.DriveLetter):" } else { $null }
            FileSystem = if ($vol) { $vol.FileSystemType } else { $null }
            Label      = if ($vol) { $vol.FileSystemLabel } else { $null }
        }
    })
    [PSCustomObject]@{
        Number     = $d.Number
        Model      = $d.FriendlyName
        Serial     = $d.SerialNumber
        BusType    = "$($d.BusType)"
        SizeBytes  = $d.Size
        Style      = "$($d.PartitionStyle)"
        Status     = "$($d.OperationalStatus)"
        Partitions = $parts
    }
}
$disks | ConvertTo-Json -Depth 3 -Compress
"#;

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                warn!(
                    stderr = %String::from_utf8_lossy(&o.stderr),
                    "PowerShell Get-Disk failed"
                );
                return Vec::new();
            }
            Err(e) => {
                warn!(error = %e, "Failed to spawn PowerShell for disk scan");
                return Vec::new();
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout = stdout.trim();
        if stdout.is_empty() {
            return Vec::new();
        }

        parse_media_json(stdout)
    }

    /// Parse the PowerShell JSON output into MediumSnapshots.
    fn parse_media_json(json_str: &str) -> Vec<MediumSnapshot> {
        // PowerShell emits a bare object (not array) when there's only one disk.
        let raw: Vec<serde_json::Value> = if json_str.starts_with('[') {
            serde_json::from_str(json_str).unwrap_or_default()
        } else {
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => vec![v],
                Err(e) => {
                    warn!(error = %e, "Failed to parse disk scan JSON");
                    return Vec::new();
                }
            }
        };

        let mut results = Vec::new();

        // Find which disk contains C:\ so we can skip the system disk
        let system_disk_number: Option<i64> = raw.iter().find_map(|disk| {
            let parts_val = disk.get("Partitions")?;
            // PowerShell emits bare object for single partition, array for multiple
            let parts: Vec<&serde_json::Value> = if let Some(arr) = parts_val.as_array() {
                arr.iter().collect()
            } else if parts_val.is_object() {
                vec![parts_val]
            } else {
                return None;
            };
            let has_c = parts.iter().any(|p| {
                p.get("DriveLetter")
                    .and_then(|v| v.as_str())
                    .map(|l| l.eq_ignore_ascii_case("C:"))
                    .unwrap_or(false)
            });
            if has_c {
                disk.get("Number")?.as_i64()
            } else {
                None
            }
        });

        for disk in &raw {
            let number = disk.get("Number").and_then(|v| v.as_i64()).unwrap_or(-1);

            // Skip system disk
            if Some(number) == system_disk_number {
                continue;
            }

            let model = disk.get("Model").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
            let serial = disk.get("Serial").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
            let bus_str = disk.get("BusType").and_then(|v| v.as_str()).unwrap_or("");
            let size = disk.get("SizeBytes").and_then(|v| v.as_u64()).unwrap_or(0);
            let style = disk.get("Style").and_then(|v| v.as_str()).unwrap_or("Unknown");
            let status = disk.get("Status").and_then(|v| v.as_str()).unwrap_or("Unknown");

            let bus_type = match bus_str {
                "USB" => BusType::Usb,
                "SATA" => BusType::Sata,
                "NVMe" => BusType::Nvme,
                "SCSI" | "SAS" => BusType::Scsi,
                "SD" | "MMC" => BusType::Mmc,
                _ => BusType::Unknown,
            };

            let removable = bus_type == BusType::Usb || bus_type == BusType::Mmc;

            let condition = if status != "Online" {
                MediumCondition::Unreadable
            } else if style == "RAW" {
                MediumCondition::Raw
            } else {
                MediumCondition::Partitioned
            };

            // Parse partitions
            let partitions = disk
                .get("Partitions")
                .and_then(|v| {
                    // Single partition → bare object, multiple → array
                    if v.is_array() {
                        v.as_array().cloned()
                    } else if v.is_object() {
                        Some(vec![v.clone()])
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
                .iter()
                .map(|p| PartitionSnapshot {
                    index: p.get("Index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    size_bytes: p.get("SizeBytes").and_then(|v| v.as_u64()).unwrap_or(0),
                    filesystem: p
                        .get("FileSystem")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty() && *s != "Unknown")
                        .map(|s| s.to_string()),
                    mount_path: p
                        .get("DriveLetter")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| format!("{}\\", s)),
                    label: p
                        .get("Label")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                })
                .collect();

            results.push(MediumSnapshot {
                device_id: format!("\\\\.\\PhysicalDrive{}", number),
                model,
                serial,
                bus_type,
                size_bytes: size,
                removable,
                condition,
                partitions,
            });
        }

        debug!(count = results.len(), "Windows media scan complete");
        results
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_usage_on_current_dir() {
        // Should work on any platform for the current directory
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_string_lossy();
        // disk_usage may return None in CI but shouldn't panic
        let _usage = disk_usage(&cwd_str);
    }

    #[test]
    fn test_volume_snapshot_clone() {
        let snap = VolumeSnapshot {
            path: "/dev/sdb1".to_string(),
            mount_path: "/mnt/usb".to_string(),
            label: Some("TEST".to_string()),
            capacity_bytes: 1_000_000,
            removable: true,
        };
        let cloned = snap.clone();
        assert_eq!(cloned.path, snap.path);
        assert_eq!(cloned.removable, snap.removable);
    }

    #[test]
    fn test_disk_usage_total() {
        let du = DiskUsage {
            used_bytes: 100,
            available_bytes: 200,
        };
        assert_eq!(du.total(), 300);
    }

    #[test]
    fn test_scan_volumes_finds_drives() {
        let volumes = scan_volumes();
        assert!(!volumes.is_empty(), "scan_volumes should find at least one volume");
    }

    /// Verify USB bus type detection via IOCTL.
    #[cfg(target_os = "windows")]
    #[test]
    fn test_system_drive_not_usb() {
        // C:\ is never USB — is_removable should return false
        assert!(!is_removable("C:\\"));
    }

    /// Run scan_media() and verify it returns structured results.
    /// Run with: cargo test --package garden-moss -- test_scan_media --nocapture
    #[test]
    fn test_scan_media() {
        let media = scan_media();
        eprintln!("\n=== scan_media() found {} media ===", media.len());
        for m in &media {
            eprintln!(
                "  {} | {} | {} | {} | {} GB | {} partitions",
                m.device_id,
                m.model.as_deref().unwrap_or("(unknown)"),
                m.bus_type,
                m.condition,
                m.size_bytes / 1_000_000_000,
                m.partitions.len(),
            );
            for p in &m.partitions {
                eprintln!(
                    "    part {} | {} | {} GB | mount={} | label={}",
                    p.index,
                    p.filesystem.as_deref().unwrap_or("(none)"),
                    p.size_bytes / 1_000_000_000,
                    p.mount_path.as_deref().unwrap_or("(none)"),
                    p.label.as_deref().unwrap_or("(none)"),
                );
            }
        }
        eprintln!("=== end ===\n");
        // System disk is filtered out, but we should find at least the non-system disks
    }
}
