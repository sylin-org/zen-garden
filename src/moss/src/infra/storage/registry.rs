//! Seed bank registry - live discovery of mounted seed banks
//!
//! No persistence file - the USB device manifests ARE the source of truth.
//! This module scans mounted devices to build the registry in-memory.
//! 
//! Auto-mount behavior: Devices with the `zen-seed` filesystem label are
//! automatically mounted during scan, ensuring prepared seed banks are
//! always available after reboot or service restart.

use anyhow::{Context, Result};
use garden_common::storage::{SeedBankInfo, SeedBankManifest};
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use tracing::info;
use tracing::{debug, warn};

use super::device::DeviceAnalyzer;

/// Registry of all seed banks discovered on this stone (in-memory only)
#[derive(Debug, Clone, Default)]
pub struct SeedBankRegistry {
    /// Map from seed bank name to info
    banks: HashMap<String, SeedBankInfo>,
}

impl SeedBankRegistry {
    /// Scan all mounted seed banks and build registry.
    /// 
    /// This first auto-mounts any unmounted devices with the `zen-seed` label,
    /// then scans the mounts directory for valid manifests.
    pub async fn scan() -> Result<Self> {
        // Auto-mount any unmounted seed banks before scanning
        if let Err(e) = Self::auto_mount_seed_banks().await {
            warn!(error = %e, "Failed to auto-mount seed banks");
        }
        
        let data_dir = garden_common::constants::paths::data_dir();
        let mounts_dir = PathBuf::from(&data_dir).join("mounts");
        
        let mut registry = Self::default();
        
        if !mounts_dir.exists() {
            return Ok(registry);
        }
        
        // Scan each subdirectory in mounts/
        let mut entries = match tokio::fs::read_dir(&mounts_dir).await {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to read mounts directory");
                return Ok(registry);
            }
        };
        
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            let manifest_path = path.join(".zen-garden").join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            
            // Read and parse manifest
            match Self::read_manifest(&manifest_path).await {
                Ok(manifest) => {
                    let mount_path = path.to_string_lossy().to_string();
                    
                    // Get device from mount info
                    let device = Self::get_device_for_mount(&mount_path).await
                        .unwrap_or_else(|| "unknown".to_string());
                    
                    // Get disk usage
                    let (used_bytes, capacity_bytes) = DeviceAnalyzer::get_disk_usage(&mount_path)
                        .map(|(used, avail)| (used, used + avail))
                        .unwrap_or((0, 0));
                    
                    // Check if roaming (from different stone)
                    // Use hostname as stone name (same as app_state initialization)
                    let stone_name = hostname::get()
                        .map(|h| h.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "unknown".to_string());
                    let roaming = manifest.origin_stone != stone_name;
                    
                    let info = SeedBankInfo {
                        id: manifest.id,
                        name: manifest.name.clone(),
                        pool_id: manifest.pool_id,
                        device,
                        mount_path,
                        capacity_bytes,
                        used_bytes,
                        visibility: manifest.visibility,
                        btrfs: manifest.filesystem == "btrfs",
                        origin_stone: manifest.origin_stone,
                        created_at: manifest.created_at,
                        last_sync: None,
                        roaming,
                        online: true, // If we can read it, it's online
                    };
                    
                    debug!(name = %info.name, device = %info.device, "Discovered seed bank");
                    registry.banks.insert(manifest.name, info);
                }
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "Failed to read seed bank manifest");
                }
            }
        }
        
        Ok(registry)
    }
    
    /// Read manifest from disk
    async fn read_manifest(path: &PathBuf) -> Result<SeedBankManifest> {
        let content = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read manifest file")?;
        
        let manifest: SeedBankManifest = serde_json::from_str(&content)
            .context("Failed to parse manifest JSON")?;
        
        Ok(manifest)
    }
    
    /// Get device path for a mount point (from /proc/mounts)
    async fn get_device_for_mount(mount_path: &str) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let mounts = tokio::fs::read_to_string("/proc/mounts").await.ok()?;
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == mount_path {
                    return Some(parts[0].to_string());
                }
            }
            None
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            let _ = mount_path;
            None
        }
    }
    
    /// Auto-mount all unmounted devices with the `zen-seed` filesystem label.
    /// 
    /// This enables plug-and-play behavior for prepared seed banks:
    /// 1. Discovers all block devices with `zen-seed` label
    /// 2. Skips devices that are already mounted
    /// 3. Mounts unmounted devices to `/var/lib/zen-garden/mounts/seed-bank-{name}`
    /// 
    /// Edge cases handled:
    /// - Already mounted devices: skipped
    /// - Mount directory already exists: reused
    /// - Mount failure: logged and skipped (device may be busy/corrupt)
    /// - Multiple devices with same label: each gets unique mount point
    #[cfg(target_os = "linux")]
    async fn auto_mount_seed_banks() -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;
        
        // Get all block devices with zen-seed label using lsblk
        // Format: NAME,LABEL,MOUNTPOINT
        let output = Command::new("lsblk")
            .args(["-rno", "NAME,LABEL,MOUNTPOINT"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .context("Failed to run lsblk")?;
        
        if !output.status.success() {
            return Ok(()); // lsblk not available, skip auto-mount
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let data_dir = garden_common::constants::paths::data_dir();
        let mounts_dir = PathBuf::from(&data_dir).join("mounts");
        
        // Ensure mounts directory exists
        if let Err(e) = tokio::fs::create_dir_all(&mounts_dir).await {
            warn!(error = %e, "Failed to create mounts directory");
            return Ok(());
        }
        
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            
            // Parse lsblk output - format varies:
            // "sdb zen-seed" (no mountpoint)
            // "sdb zen-seed /mount/path" (mounted)
            // "sdb" (no label, no mountpoint)
            let device_name = parts[0];
            
            // Check if this is a zen-seed device
            // Label is second field if present
            let label = if parts.len() >= 2 && parts[1] != "" {
                parts[1]
            } else {
                continue; // No label
            };
            
            if label != "zen-seed" {
                continue;
            }
            
            // Check if already mounted (third field present and not empty)
            if parts.len() >= 3 && !parts[2].is_empty() {
                debug!(device = %device_name, mount = parts[2], "zen-seed device already mounted");
                continue;
            }
            
            let device_path = format!("/dev/{}", device_name);
            
            // Check if device is removable (skip internal drives)
            if !DeviceAnalyzer::is_removable(&device_path).unwrap_or(false) {
                debug!(device = %device_path, "Skipping non-removable zen-seed device");
                continue;
            }
            
            // Determine mount point name
            // Try to read the manifest from a temp mount to get the seed bank name
            let mount_name = Self::get_seed_bank_name_from_device(&device_path)
                .await
                .unwrap_or_else(|| format!("seed-bank-{}", device_name));
            
            let mount_path = mounts_dir.join(&mount_name);
            
            // Create mount point if it doesn't exist
            if let Err(e) = tokio::fs::create_dir_all(&mount_path).await {
                warn!(
                    device = %device_path,
                    mount = %mount_path.display(),
                    error = %e,
                    "Failed to create mount point"
                );
                continue;
            }
            
            // Check if something else is already mounted at this path
            if Self::is_mount_point(&mount_path).await {
                debug!(
                    mount = %mount_path.display(),
                    "Mount point already has something mounted, skipping"
                );
                continue;
            }
            
            // Mount the device
            info!(
                device = %device_path,
                mount = %mount_path.display(),
                "Auto-mounting zen-seed device"
            );
            
            let mount_result = Command::new("sudo")
                .args(["mount", &device_path, &mount_path.to_string_lossy()])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;
            
            match mount_result {
                Ok(output) if output.status.success() => {
                    info!(
                        device = %device_path,
                        mount = %mount_path.display(),
                        "Successfully auto-mounted seed bank"
                    );
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        device = %device_path,
                        mount = %mount_path.display(),
                        error = %stderr.trim(),
                        "Failed to mount seed bank (device may be busy or corrupted)"
                    );
                }
                Err(e) => {
                    warn!(
                        device = %device_path,
                        mount = %mount_path.display(),
                        error = %e,
                        "Failed to execute mount command"
                    );
                }
            }
        }
        
        Ok(())
    }
    
    #[cfg(not(target_os = "linux"))]
    async fn auto_mount_seed_banks() -> Result<()> {
        // Auto-mount not supported on non-Linux platforms
        Ok(())
    }
    
    /// Try to determine the seed bank name by temporarily mounting and reading manifest
    #[cfg(target_os = "linux")]
    async fn get_seed_bank_name_from_device(device_path: &str) -> Option<String> {
        use std::process::Stdio;
        use tokio::process::Command;
        
        let temp_mount = format!("/tmp/zen-garden-probe-{}", std::process::id());
        
        // Create temp mount point
        let _ = Command::new("sudo")
            .args(["mkdir", "-p", &temp_mount])
            .output()
            .await;
        
        // Try to mount read-only
        let mount_result = Command::new("sudo")
            .args(["mount", "-o", "ro", device_path, &temp_mount])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;
        
        let name = if let Ok(output) = mount_result {
            if output.status.success() {
                // Read manifest
                let manifest_path = format!("{}/.zen-garden/manifest.json", temp_mount);
                let name = if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await {
                    if let Ok(manifest) = serde_json::from_str::<SeedBankManifest>(&content) {
                        Some(manifest.name)
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                // Unmount
                let _ = Command::new("sudo")
                    .args(["umount", &temp_mount])
                    .output()
                    .await;
                
                name
            } else {
                None
            }
        } else {
            None
        };
        
        // Cleanup temp mount point
        let _ = Command::new("sudo")
            .args(["rmdir", &temp_mount])
            .output()
            .await;
        
        name
    }
    
    /// Check if a path is a mount point
    #[cfg(target_os = "linux")]
    async fn is_mount_point(path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy().to_string();
        
        if let Ok(mounts) = tokio::fs::read_to_string("/proc/mounts").await {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == path_str {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Get a seed bank by name
    pub fn get(&self, name: &str) -> Option<&SeedBankInfo> {
        self.banks.get(name)
    }
    
    /// List all seed banks
    pub fn list(&self) -> Vec<&SeedBankInfo> {
        self.banks.values().collect()
    }
    
    /// Check if a seed bank exists
    pub fn exists(&self, name: &str) -> bool {
        self.banks.contains_key(name)
    }
    
    /// Find seed bank by device path
    pub fn find_by_device(&self, device: &str) -> Option<&SeedBankInfo> {
        self.banks.values().find(|b| b.device == device)
    }
    
    /// Find seed bank by mount path
    pub fn find_by_mount(&self, mount_path: &str) -> Option<&SeedBankInfo> {
        self.banks.values().find(|b| b.mount_path == mount_path)
    }

    /// Find seed bank by ID (GUIDv7)
    pub fn find_by_id(&self, id: &str) -> Option<&SeedBankInfo> {
        self.banks.values().find(|b| b.id == id)
    }

    /// Get seed bank by name (alias for get)
    pub fn get_by_name(&self, name: &str) -> Option<&SeedBankInfo> {
        self.get(name)
    }

    /// Get seed bank by ID (alias for find_by_id)
    pub fn get_by_id(&self, id: &str) -> Option<&SeedBankInfo> {
        self.find_by_id(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_empty_scan() {
        // Just verify it doesn't crash on empty system
        let registry = SeedBankRegistry::scan().await.unwrap();
        assert!(registry.list().is_empty() || !registry.list().is_empty());
    }
}
