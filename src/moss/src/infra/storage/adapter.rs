//! Storage adapter trait (STORAGE-0009)
//!
//! Abstracts device lifecycle: how a storage medium is discovered, mounted,
//! and unmounted. Each adapter type handles one class of storage medium.
//!
//! The adapter is responsible for:
//! - **Discovery**: finding available devices/paths
//! - **Mounting**: making a device accessible at a filesystem path
//! - **Unmounting**: safely detaching a device
//! - **Health**: medium-specific health assessment
//!
//! After mounting, the adapter hands off to `StorageDevice` for ongoing
//! lifecycle management (health ticks, self-healing remount, capacity tracking).

use std::path::PathBuf;

use anyhow::Result;

// ============================================================================
// Adapter trait
// ============================================================================

/// Type of storage adapter — determines discovery and mount behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterType {
    /// USB/removable block device. Hot-plug detection, auto-mount.
    Usb,
    /// NAS mount (NFS/SMB). Persistent mount, reconnect on failure.
    Nas,
    /// Local filesystem path. Always available, no mount/unmount.
    Path,
}

impl std::fmt::Display for AdapterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb => write!(f, "usb"),
            Self::Nas => write!(f, "nas"),
            Self::Path => write!(f, "path"),
        }
    }
}

/// A discovered storage candidate that can be mounted.
#[derive(Debug, Clone)]
pub struct StorageCandidate {
    /// Adapter type that discovered this candidate.
    pub adapter_type: AdapterType,
    /// Device identifier (e.g., `/dev/sdb1` for USB, NFS URI for NAS).
    pub device: String,
    /// Optional label from the filesystem.
    pub label: Option<String>,
    /// Total capacity in bytes (if known before mount).
    pub capacity_bytes: Option<u64>,
    /// Filesystem type (if detectable before mount).
    pub filesystem: Option<String>,
}

/// Device lifecycle abstraction.
///
/// Each implementation handles one class of storage medium. Adding NAS
/// or local path support requires only a new implementation — no changes
/// to `StorageService` or domain logic.
#[async_trait::async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Adapter type identifier.
    fn adapter_type(&self) -> AdapterType;

    /// Discover available candidates that could be mounted.
    async fn discover(&self) -> Result<Vec<StorageCandidate>>;

    /// Mount a device at the given path.
    ///
    /// The `mount_path` directory will be created by the caller if needed.
    /// Returns the actual mount path (may differ from requested on some platforms).
    async fn mount(&self, device: &str, mount_path: &std::path::Path) -> Result<PathBuf>;

    /// Unmount a device.
    async fn unmount(&self, mount_path: &std::path::Path) -> Result<()>;

    /// Check whether a specific device is currently available (plugged in, reachable).
    async fn is_available(&self, device: &str) -> bool;
}

// ============================================================================
// USB adapter
// ============================================================================

/// USB/removable block device adapter.
///
/// Wraps existing device detection (`DeviceAnalyzer`) and mount logic.
/// Linux-only for the mount/unmount operations; detection is cross-platform
/// via fallback methods.
pub struct UsbAdapter;

#[async_trait::async_trait]
impl StorageAdapter for UsbAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Usb
    }

    async fn discover(&self) -> Result<Vec<StorageCandidate>> {
        let devices = super::list_unmounted_removable_devices()?;
        Ok(devices
            .into_iter()
            .map(|d| StorageCandidate {
                adapter_type: AdapterType::Usb,
                device: d.device,
                label: d.label,
                capacity_bytes: Some(d.capacity_bytes),
                filesystem: None,
            })
            .collect())
    }

    async fn mount(&self, device: &str, mount_path: &std::path::Path) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            tokio::fs::create_dir_all(mount_path).await?;
            let mount_str = mount_path.to_string_lossy().to_string();

            let output = tokio::process::Command::new("sudo")
                .args(["mount", device, &mount_str])
                .output()
                .await?;

            if output.status.success() {
                Ok(mount_path.to_path_buf())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("mount {} -> {} failed: {}", device, mount_str, stderr.trim())
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (device, mount_path);
            anyhow::bail!("USB mount not supported on this platform")
        }
    }

    async fn unmount(&self, mount_path: &std::path::Path) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let mount_str = mount_path.to_string_lossy().to_string();

            let output = tokio::process::Command::new("sudo")
                .args(["umount", &mount_str])
                .output()
                .await?;

            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("umount {} failed: {}", mount_str, stderr.trim())
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = mount_path;
            anyhow::bail!("USB unmount not supported on this platform")
        }
    }

    async fn is_available(&self, device: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new(device).exists()
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = device;
            false
        }
    }
}

// ============================================================================
// NAS adapter (NFS/SMB persistent mount)
// ============================================================================

/// NAS mount adapter — NFS or SMB network shares.
///
/// Mounts via `mount -t nfs` or `mount -t cifs` depending on the URI scheme.
/// Discovery is configuration-based (not automatic). Mount failures trigger
/// reconnect attempts from the health tick.
///
/// Device string format:
/// - NFS: `nfs://host/export/path` or `host:/export/path`
/// - SMB: `smb://host/share` or `//host/share`
pub struct NasAdapter;

impl NasAdapter {
    /// Parse a NAS device string into (fs_type, mount_source).
    ///
    /// Returns e.g. `("nfs", "192.168.1.5:/volume1/share")` or
    /// `("cifs", "//nas.local/photos")`.
    fn parse_device(device: &str) -> Result<(&'static str, String)> {
        let trimmed = device.trim();

        // NFS: nfs://host/path or host:/path
        if let Some(rest) = trimmed.strip_prefix("nfs://") {
            let source = if rest.contains(':') {
                rest.to_string()
            } else {
                // nfs://host/path -> host:/path
                rest.replacen('/', ":/", 1)
            };
            return Ok(("nfs", source));
        }
        if trimmed.contains(":/") && !trimmed.starts_with('/') && !trimmed.starts_with("//") {
            // Already in host:/path form
            return Ok(("nfs", trimmed.to_string()));
        }

        // SMB: smb://host/share or //host/share
        if let Some(rest) = trimmed.strip_prefix("smb://") {
            return Ok(("cifs", format!("//{}", rest)));
        }
        if trimmed.starts_with("//") {
            return Ok(("cifs", trimmed.to_string()));
        }

        anyhow::bail!(
            "Unrecognized NAS device format: {}. Expected nfs://host/path, host:/path, smb://host/share, or //host/share",
            trimmed
        )
    }
}

#[async_trait::async_trait]
impl StorageAdapter for NasAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Nas
    }

    async fn discover(&self) -> Result<Vec<StorageCandidate>> {
        // NAS adapters don't auto-discover — they're configured explicitly
        Ok(Vec::new())
    }

    async fn mount(&self, device: &str, mount_path: &std::path::Path) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            let (fs_type, source) = Self::parse_device(device)?;

            tokio::fs::create_dir_all(mount_path).await?;
            let mount_str = mount_path.to_string_lossy().to_string();

            // Build mount options based on filesystem type
            let options = match fs_type {
                "nfs" => "soft,timeo=50,retrans=3",
                "cifs" => "guest,vers=3.0,sec=none",
                _ => "",
            };

            let mut args = vec!["-t", fs_type];
            if !options.is_empty() {
                args.extend(["-o", options]);
            }
            args.extend([&*source, &mount_str]);

            let output = tokio::process::Command::new("sudo")
                .arg("mount")
                .args(&args)
                .output()
                .await?;

            if output.status.success() {
                Ok(mount_path.to_path_buf())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("mount -t {} {} -> {} failed: {}", fs_type, source, mount_str, stderr.trim())
            }
        }

        #[cfg(target_os = "windows")]
        {
            let (_fs_type, source) = Self::parse_device(device)?;

            // On Windows, use net use for SMB or mount for NFS
            if source.starts_with("//") || source.starts_with("\\\\") {
                // SMB — use net use to map, then access via UNC or mount point
                let unc = source.replace('/', "\\");
                let mount_str = mount_path.to_string_lossy().to_string();

                // Create mount directory
                tokio::fs::create_dir_all(mount_path).await?;

                // Use mklink /D to create a junction to the UNC path
                let output = tokio::process::Command::new("cmd")
                    .args(["/c", "mklink", "/D", &mount_str, &unc])
                    .output()
                    .await?;

                if output.status.success() {
                    Ok(mount_path.to_path_buf())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("Failed to link {} -> {}: {}", mount_str, unc, stderr.trim())
                }
            } else {
                anyhow::bail!("NFS mount not supported on Windows; use SMB (//host/share)")
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = (device, mount_path);
            anyhow::bail!("NAS mount not supported on this platform")
        }
    }

    async fn unmount(&self, mount_path: &std::path::Path) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let mount_str = mount_path.to_string_lossy().to_string();

            // Lazy unmount for NAS — avoids blocking on hung shares
            let output = tokio::process::Command::new("sudo")
                .args(["umount", "-l", &mount_str])
                .output()
                .await?;

            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("umount -l {} failed: {}", mount_str, stderr.trim())
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Remove junction/symlink
            let mount_str = mount_path.to_string_lossy().to_string();
            let output = tokio::process::Command::new("cmd")
                .args(["/c", "rmdir", &mount_str])
                .output()
                .await?;

            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("rmdir {} failed: {}", mount_str, stderr.trim())
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = mount_path;
            anyhow::bail!("NAS unmount not supported on this platform")
        }
    }

    async fn is_available(&self, device: &str) -> bool {
        // Try to parse and ping the host
        let host = match Self::parse_device(device) {
            Ok((_, source)) => {
                // Extract host from source
                if let Some(rest) = source.strip_prefix("//") {
                    rest.split('/').next().unwrap_or("").to_string()
                } else {
                    source.split(':').next().unwrap_or("").to_string()
                }
            }
            Err(_) => return false,
        };

        if host.is_empty() {
            return false;
        }

        // Quick TCP probe on common NFS/SMB ports
        let addrs = [
            (host.as_str(), 445u16),  // SMB
            (host.as_str(), 2049u16), // NFS
        ];

        for (h, port) in &addrs {
            if let Ok(addr) = tokio::net::lookup_host(format!("{}:{}", h, port)).await {
                for a in addr {
                    if tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        tokio::net::TcpStream::connect(a),
                    )
                    .await
                    .is_ok()
                    {
                        return true;
                    }
                }
            }
        }

        false
    }
}

// ============================================================================
// Path adapter (always-available local path)
// ============================================================================

/// Local filesystem path adapter.
///
/// The simplest adapter — the "device" is a directory path that's always
/// available. No mount/unmount needed. Used for bringing existing directories
/// under Zen Garden management (e.g., a NAS volume already mounted via fstab).
pub struct PathAdapter;

#[async_trait::async_trait]
impl StorageAdapter for PathAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Path
    }

    async fn discover(&self) -> Result<Vec<StorageCandidate>> {
        // Path adapters don't discover — they're configured explicitly
        Ok(Vec::new())
    }

    async fn mount(&self, device: &str, _mount_path: &std::path::Path) -> Result<PathBuf> {
        // The "device" IS the path — no mounting needed
        let path = PathBuf::from(device);
        if !path.is_dir() {
            anyhow::bail!("Path {} does not exist or is not a directory", device);
        }
        Ok(path)
    }

    async fn unmount(&self, _mount_path: &std::path::Path) -> Result<()> {
        // No-op for path adapter
        Ok(())
    }

    async fn is_available(&self, device: &str) -> bool {
        std::path::Path::new(device).is_dir()
    }
}
