//! Device analysis and eligibility checking
//!
//! Examines storage devices to determine if they can be prepared as seed banks.
//! Uses multiple detection methods for robust USB/removable device identification.

use anyhow::{Context, Result};
use garden_common::storage::{DeviceState, StorageDetectedInfo};
use std::path::Path;
use tracing::debug;

/// Analyzes a storage device and returns its information
pub struct DeviceAnalyzer;

impl DeviceAnalyzer {
    /// Check if a device is removable (USB, SD card, etc.)
    /// Uses multiple detection methods for reliability:
    /// 1. sysfs removable flag (quick but not always set for USB)
    /// 2. USB bus detection via sysfs device symlink (canonical path)
    /// 3. DRIVER check in uevent
    pub fn is_removable(device_path: &str) -> Result<bool> {
        let base_name = Self::get_base_device_name(device_path);
        
        // Method 1: Check sysfs removable flag
        let removable_path = format!("/sys/block/{}/removable", base_name);
        if let Ok(content) = std::fs::read_to_string(&removable_path) {
            if content.trim() == "1" {
                debug!(device = %device_path, method = "removable_flag", "Device is removable");
                return Ok(true);
            }
        }
        
        // Method 2: Check if device is on USB bus via canonical path
        // The canonical path will be like /sys/devices/pci.../usb1/1-3/.../host/target/...
        let device_sysfs_path = format!("/sys/block/{}/device", base_name);
        if let Ok(canonical) = std::fs::canonicalize(&device_sysfs_path) {
            let path_str = canonical.to_string_lossy();
            if path_str.contains("/usb") || path_str.contains("/mmc") {
                debug!(device = %device_path, method = "canonical_path", path = %path_str, "Device is on USB bus");
                return Ok(true);
            }
        }
        
        // Method 3: Check subsystem via uevent
        let uevent_path = format!("/sys/block/{}/device/uevent", base_name);
        if let Ok(content) = std::fs::read_to_string(&uevent_path) {
            if content.contains("DRIVER=usb-storage") || content.contains("DRIVER=uas") {
                debug!(device = %device_path, method = "uevent", "Device uses USB storage driver");
                return Ok(true);
            }
        }
        
        debug!(device = %device_path, "Device is not removable");
        Ok(false)
    }
    
    /// Extract base device name (e.g., "sdb" from "/dev/sdb1")
    fn get_base_device_name(device_path: &str) -> String {
        let device_name = Path::new(device_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        // Remove partition number suffix
        device_name
            .chars()
            .take_while(|c| !c.is_ascii_digit())
            .collect()
    }
    
    /// Get device capacity in bytes from sysfs
    pub fn get_capacity(device_path: &str) -> Result<u64> {
        let device_name = Path::new(device_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        // For partitions, get size from partition sysfs
        let size_path = format!("/sys/class/block/{}/size", device_name);
        
        if let Ok(content) = std::fs::read_to_string(&size_path) {
            // Size is in 512-byte sectors
            let sectors: u64 = content.trim().parse().unwrap_or(0);
            return Ok(sectors * 512);
        }
        
        // Fallback: try blockdev command
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("blockdev")
                .args(["--getsize64", device_path])
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    let size_str = String::from_utf8_lossy(&output.stdout);
                    if let Ok(size) = size_str.trim().parse() {
                        return Ok(size);
                    }
                }
            }
        }
        
        Ok(0)
    }
    
    /// Get device label from filesystem
    #[allow(unused_variables)]
    pub fn get_label(device_path: &str) -> Option<String> {
        // Try lsblk for label
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("lsblk")
                .args(["-no", "LABEL", device_path])
                .output()
                .ok()?;
            
            if output.status.success() {
                let label = String::from_utf8_lossy(&output.stdout);
                let label = label.trim();
                if !label.is_empty() {
                    return Some(label.to_string());
                }
            }
        }
        
        None
    }
    
    /// Check if device is mounted and get mount path
    pub fn get_mount_path(device_path: &str) -> Option<String> {
        // Parse /proc/mounts
        if let Ok(content) = std::fs::read_to_string("/proc/mounts") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == device_path {
                    return Some(parts[1].to_string());
                }
            }
        }
        
        None
    }
    
    /// Check if mount path is in allowed locations
    pub fn is_allowed_mount(mount_path: &str) -> bool {
        mount_path.starts_with("/mnt/") ||
        mount_path.starts_with("/media/") ||
        mount_path.starts_with("/run/media/") ||
        mount_path.starts_with("/var/lib/zen-garden/mounts/") ||
        mount_path.starts_with("/var/lib/garden-moss/mounts/")
    }
    
    /// Get disk usage for a mounted path
    /// Returns (used_bytes, available_bytes) or None if unavailable
    #[allow(unused_variables)]
    pub fn get_disk_usage(mount_path: &str) -> Option<(u64, u64)> {
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("df")
                .args(["-B1", "--output=used,avail", mount_path])
                .output()
                .ok()?;
            
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Skip header line, parse data line
                let lines: Vec<&str> = stdout.lines().collect();
                if lines.len() >= 2 {
                    let parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let used: u64 = parts[0].parse().ok()?;
                        let avail: u64 = parts[1].parse().ok()?;
                        return Some((used, avail));
                    }
                }
            }
        }
        None
    }
    
    /// Determine device state by examining filesystem
    #[allow(unused_variables)]
    pub fn determine_state(device_path: &str, mount_path: Option<&str>) -> Result<DeviceState> {
        // Check if device has a filesystem
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("blkid")
                .args(["-o", "value", "-s", "TYPE", device_path])
                .output()
                .context("Failed to run blkid")?;
            
            if !output.status.success() || output.stdout.is_empty() {
                // No filesystem detected - check if partitioned
                let output = std::process::Command::new("blkid")
                    .args(["-p", device_path])
                    .output()
                    .context("Failed to run blkid -p")?;
                
                if output.stdout.is_empty() {
                    return Ok(DeviceState::Unpartitioned);
                }
                return Ok(DeviceState::Unformatted);
            }
        }
        
        // Has filesystem - check contents if mounted
        if let Some(mount) = mount_path {
            return Self::check_mount_contents(mount);
        }
        
        // Not mounted - try to mount temporarily to inspect
        #[cfg(target_os = "linux")]
        {
            let temp_mount = format!("/tmp/zen-garden-inspect-{}", std::process::id());
            
            // Create temp mount point (use sudo for mkdir in case /tmp has permissions issues)
            let _ = std::process::Command::new("sudo")
                .args(["mkdir", "-p", &temp_mount])
                .output();
            
            if Path::new(&temp_mount).exists() {
                // Try to mount read-only with sudo
                let mount_result = std::process::Command::new("sudo")
                    .args(["mount", "-o", "ro", device_path, &temp_mount])
                    .output();
                
                if let Ok(output) = mount_result {
                    if output.status.success() {
                        let result = Self::check_mount_contents(&temp_mount);
                        
                        // Always unmount with sudo
                        let _ = std::process::Command::new("sudo")
                            .args(["umount", &temp_mount])
                            .output();
                        let _ = std::process::Command::new("sudo")
                            .args(["rmdir", &temp_mount])
                            .output();
                        
                        return result;
                    }
                }
                let _ = std::process::Command::new("sudo")
                    .args(["rmdir", &temp_mount])
                    .output();
            }
        }
        
        // Could not mount - assume has filesystem but treat as potentially empty
        // This is safer than assuming HasData which blocks preparation
        Ok(DeviceState::Unformatted)
    }
    
    /// Check contents of a mounted filesystem
    fn check_mount_contents(mount_path: &str) -> Result<DeviceState> {
        let mount_dir = Path::new(mount_path);
        
        // Check for existing seed bank
        let zen_dir = mount_dir.join(".zen-garden");
        if zen_dir.exists() {
            // Validate manifest integrity before reporting as Prepared
            if Self::validate_manifest(&zen_dir).is_ok() {
                return Ok(DeviceState::Prepared);
            } else {
                // Corrupt manifest - treat as having data (requires manual intervention)
                debug!("Corrupt or incomplete manifest at {:?}", zen_dir);
                return Ok(DeviceState::HasData);
            }
        }
        
        // Check if empty (ignoring hidden system files)
        let has_visible_files = std::fs::read_dir(mount_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| {
                        let name = e.file_name();
                        let name_str = name.to_string_lossy();
                        // Ignore common system hidden files
                        !name_str.starts_with('.') && 
                        name_str != "System Volume Information" &&
                        name_str != "$RECYCLE.BIN"
                    })
            })
            .unwrap_or(false);
        
        if has_visible_files {
            return Ok(DeviceState::HasData);
        }
        
        Ok(DeviceState::Empty)
    }
    
    /// Validate seed bank manifest integrity
    /// 
    /// Returns Ok if manifest is valid and complete, Err if corrupt or incomplete.
    /// This prevents treating partially-written seed banks as valid.
    pub fn validate_manifest(zen_dir: &Path) -> Result<garden_common::storage::SeedBankManifest> {
        let manifest_path = zen_dir.join("manifest.json");
        
        if !manifest_path.exists() {
            anyhow::bail!("Manifest file does not exist");
        }
        
        let content = std::fs::read_to_string(&manifest_path)
            .context("Failed to read manifest file")?;
        
        let manifest: garden_common::storage::SeedBankManifest = serde_json::from_str(&content)
            .context("Manifest JSON is corrupt or incomplete")?;
        
        // Validate required fields
        if manifest.id.is_empty() {
            anyhow::bail!("Manifest missing id field");
        }
        if manifest.name.is_empty() {
            anyhow::bail!("Manifest missing name field");
        }
        if manifest.origin_stone.is_empty() {
            anyhow::bail!("Manifest missing origin_stone field");
        }
        
        // Check subdirectories exist
        if !zen_dir.join("blobs").exists() {
            anyhow::bail!("Missing blobs directory");
        }
        if !zen_dir.join("journal").exists() {
            anyhow::bail!("Missing journal directory");
        }
        
        Ok(manifest)
    }
}

/// Analyze a device and return full StorageDetectedInfo
pub fn analyze_device(device_path: &str) -> Result<StorageDetectedInfo> {
    let removable = DeviceAnalyzer::is_removable(device_path)
        .unwrap_or(false);
    
    let capacity_bytes = DeviceAnalyzer::get_capacity(device_path)
        .unwrap_or(0);
    
    let label = DeviceAnalyzer::get_label(device_path);
    let mount_path = DeviceAnalyzer::get_mount_path(device_path);
    
    let state = DeviceAnalyzer::determine_state(device_path, mount_path.as_deref())
        .unwrap_or(DeviceState::HasData);
    
    // Determine eligibility
    let mut eligible = state.is_eligible();
    let mut ineligible_reason = None;
    
    if !removable {
        eligible = false;
        ineligible_reason = Some("Device is not removable".to_string());
    } else if let Some(ref mount) = mount_path {
        if !DeviceAnalyzer::is_allowed_mount(mount) {
            eligible = false;
            ineligible_reason = Some(format!("Mount path {} is not in allowed location", mount));
        }
    }
    
    if !state.is_eligible() && ineligible_reason.is_none() {
        ineligible_reason = Some(format!("Device state is {}", state));
    }
    
    Ok(StorageDetectedInfo {
        device: device_path.to_string(),
        mount_path,
        label,
        capacity_bytes,
        state,
        eligible,
        removable,
        ineligible_reason,
    })
}

/// List all USB/removable storage partitions on the system
/// This is the main entry point for device discovery - scans all partitions
/// and uses robust USB detection (not relying on unreliable RM flag)
pub fn list_usb_partitions() -> Result<Vec<StorageDetectedInfo>> {
    let mut results = Vec::new();
    
    // Read /sys/block to get all block devices
    let sys_block = Path::new("/sys/block");
    let entries = std::fs::read_dir(sys_block)
        .context("Failed to read /sys/block")?;
    
    for entry in entries.filter_map(|e| e.ok()) {
        let device_name = entry.file_name();
        let device_name = device_name.to_string_lossy();
        
        // Skip non-disk devices (loop, dm, sr, ram, etc.)
        if !device_name.starts_with("sd") && !device_name.starts_with("nvme") {
            continue;
        }
        
        let device_path = format!("/dev/{}", device_name);
        
        // Check if this base device is removable/USB
        if !DeviceAnalyzer::is_removable(&device_path).unwrap_or(false) {
            debug!(device = %device_path, "Skipping non-removable device");
            continue;
        }
        
        debug!(device = %device_path, "Found removable/USB device, scanning partitions");
        
        // Find partitions for this device
        let device_sys_path = entry.path();
        if let Ok(contents) = std::fs::read_dir(&device_sys_path) {
            for part_entry in contents.filter_map(|e| e.ok()) {
                let part_name = part_entry.file_name();
                let part_name = part_name.to_string_lossy();
                
                // Partitions are named like sdb1, sdb2, nvme0n1p1
                if part_name.starts_with(&*device_name) && part_name != device_name {
                    let part_path = format!("/dev/{}", part_name);
                    
                    match analyze_device(&part_path) {
                        Ok(info) => {
                            debug!(
                                device = %part_path, 
                                removable = info.removable,
                                eligible = info.eligible,
                                state = ?info.state,
                                "Analyzed partition"
                            );
                            results.push(info);
                        }
                        Err(e) => {
                            debug!(device = %part_path, error = %e, "Failed to analyze partition");
                        }
                    }
                }
            }
        }
        
        // If no partitions found, check if the device itself has a filesystem
        if results.iter().all(|r| !r.device.starts_with(&device_path) || r.device == device_path) {
            match analyze_device(&device_path) {
                Ok(info) if info.state != DeviceState::Unpartitioned => {
                    debug!(device = %device_path, state = ?info.state, "Whole disk has filesystem");
                    results.push(info);
                }
                _ => {}
            }
        }
    }
    
    debug!(count = results.len(), "Total USB partitions found");
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_allowed_mount_paths() {
        assert!(DeviceAnalyzer::is_allowed_mount("/mnt/usb"));
        assert!(DeviceAnalyzer::is_allowed_mount("/media/user/USB"));
        assert!(DeviceAnalyzer::is_allowed_mount("/run/media/user/USB"));
        assert!(!DeviceAnalyzer::is_allowed_mount("/home/user/usb"));
        assert!(!DeviceAnalyzer::is_allowed_mount("/var/lib/data"));
    }
}
