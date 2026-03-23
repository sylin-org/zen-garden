//! Volume monitoring trait and platform-specific implementations (STORAGE-0014)
//!
//! Each platform implements `VolumeMonitor` which detects physical storage
//! presence changes and measures occupancy BEFORE emitting events.
//! The monitor knows nothing about manifests, names, roles, or domain events.

use std::path::PathBuf;

/// Disk metrics measured at detection time.
#[derive(Debug, Clone, Copy)]
pub struct StorageMetrics {
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

/// Physical storage event — raw facts from the OS, no domain knowledge.
#[derive(Debug, Clone)]
pub enum PhysicalStorageEvent {
    /// A volume became accessible. Metrics are measured before emission.
    Connected {
        /// Device identifier: `/dev/sdb1` on Linux, `E:\` on Windows.
        device_path: String,
        /// Where the volume's content is accessible.
        mount_path: PathBuf,
        label: Option<String>,
        capacity_bytes: u64,
        used_bytes: u64,
        removable: bool,
    },
    /// A volume is no longer accessible.
    ///
    /// `path` is the device identifier: `/dev/sdb1` on Linux, `E:\` on Windows.
    Disconnected { path: String },
}

/// Platform-specific volume monitor.
///
/// Each implementation detects device presence via OS-appropriate mechanisms
/// and measures disk usage before emitting events through the channel.
pub trait VolumeMonitor: Send + Sync {
    /// Start monitoring. Spawns background tasks that send events through `tx`.
    /// Stops when `token` is cancelled.
    fn start(
        self: Box<Self>,
        tx: tokio::sync::mpsc::Sender<PhysicalStorageEvent>,
        token: tokio_util::sync::CancellationToken,
    );
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

/// Build the platform-appropriate volume monitor.
pub fn build_monitor() -> Box<dyn VolumeMonitor> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxVolumeMonitor)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsVolumeMonitor)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Box::new(NoopVolumeMonitor)
    }
}

/// Fallback monitor for unsupported platforms.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
struct NoopVolumeMonitor;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl VolumeMonitor for NoopVolumeMonitor {
    fn start(
        self: Box<Self>,
        _tx: tokio::sync::mpsc::Sender<PhysicalStorageEvent>,
        _token: tokio_util::sync::CancellationToken,
    ) {
        tracing::warn!("Volume monitor not supported on this platform");
    }
}
