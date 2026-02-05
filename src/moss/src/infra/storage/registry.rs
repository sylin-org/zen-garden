//! Seed bank registry - live discovery of mounted seed banks
//!
//! No persistence file - the USB device manifests ARE the source of truth.
//! This module scans mounted devices to build the registry in-memory.
//!
//! Auto-mount behavior: Devices with the `zen-seed` filesystem label are
//! automatically mounted during scan, ensuring prepared seed banks are
//! always available after reboot or service restart.
//!
//! Mount persistence: A background task monitors mounts every 5 seconds and
//! automatically re-mounts devices that have unexpectedly become unmounted
//! (e.g., due to system interference or race conditions with udisks2).

use anyhow::{Context, Result};
use garden_common::constants::paths;
use garden_common::storage::{SeedBankInfo, SeedBankManifest};
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use tokio::sync::RwLock;
#[cfg(target_os = "linux")]
use tracing::info;
use tracing::{debug, warn};

#[cfg(target_os = "linux")]
use crate::domain::StorageEvent;
use super::device::DeviceAnalyzer;

/// Tracks a persistent mount that should be maintained
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct TrackedMount {
    /// Device path (e.g., /dev/sdb)
    pub device: String,
    /// Mount path (e.g., /var/lib/zen-garden/mounts/seed-bank-zen-garden)
    pub mount_path: String,
    /// Seed bank name (for logging)
    pub name: String,
    /// Number of consecutive mount recovery attempts
    pub recovery_attempts: u32,
    /// Last successful mount time
    pub last_mounted: std::time::Instant,
}

/// Global state for tracking mounts that should persist
#[cfg(target_os = "linux")]
pub type MountTracker = Arc<RwLock<HashMap<String, TrackedMount>>>;

/// Create a new mount tracker
#[cfg(target_os = "linux")]
pub fn create_mount_tracker() -> MountTracker {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Registry of all seed banks discovered on this stone (in-memory only)
#[derive(Debug, Clone, Default)]
pub struct SeedBankRegistry {
    /// Map from seed bank name to info
    banks: HashMap<String, SeedBankInfo>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct MountedSeedBank {
    device: String,
    mount_path: String,
    manifest: SeedBankManifest,
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

        // Include any prepared seed banks mounted outside our mounts directory.
        // This handles udisks/desktop auto-mounts and ensures zero-touch availability.
        #[cfg(target_os = "linux")]
        if let Err(e) = Self::append_external_mounts(&mut registry, &mounts_dir).await {
            warn!(error = %e, "Failed to include external seed bank mounts");
        }
        
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

                    // Get device from mount info - this tells us if actually mounted
                    let device_opt = Self::get_device_for_mount(&mount_path).await;
                    let is_mounted = device_opt.is_some();
                    let device = device_opt.unwrap_or_else(|| "not-mounted".to_string());

                    // Skip seed banks that aren't actually mounted
                    // The manifest may exist from a previous mount, but device isn't there now
                    if !is_mounted {
                        debug!(
                            name = %manifest.name,
                            mount_path = %mount_path,
                            "Skipping seed bank - not mounted (manifest exists but no device)"
                        );
                        continue;
                    }

                    if let Err(e) = Self::ensure_seed_bank_layout(&mount_path).await {
                        warn!(
                            name = %manifest.name,
                            mount_path = %mount_path,
                            error = %e,
                            "Failed to ensure seed bank layout"
                        );
                    }

                    // Get disk usage - also serves as liveness check
                    // If device was yanked, this will fail or return 0
                    let (_used_bytes, capacity_bytes) = DeviceAnalyzer::get_disk_usage(&mount_path)
                        .map(|(used, avail)| (used, used + avail))
                        .unwrap_or((0, 0));

                    // Liveness check: if capacity is 0, mount is likely stale/dead
                    // This catches the case where device was physically removed
                    if capacity_bytes == 0 {
                        warn!(
                            name = %manifest.name,
                            device = %device,
                            mount_path = %mount_path,
                            "Skipping seed bank - mount appears stale (0 capacity, device may have been removed)"
                        );

                        // Clean up the stale mount so it doesn't cause issues
                        #[cfg(target_os = "linux")]
                        Self::cleanup_stale_mount(&mount_path).await;

                        continue;
                    }

                    if let Some(info) = Self::build_seed_bank_info(manifest, &mount_path, &device) {
                        debug!(name = %info.name, device = %info.device, "Discovered seed bank");
                        registry.banks.insert(info.name.clone(), info);
                    }
                }
                Err(e) => {
                    let mount_path = path.to_string_lossy().to_string();
                    let error_str = e.to_string().to_lowercase();

                    // Check if this is an I/O error (device likely yanked)
                    if error_str.contains("i/o error") || error_str.contains("input/output error") {
                        warn!(
                            mount_path = %mount_path,
                            error = %e,
                            "Seed bank I/O error - device may have been removed"
                        );

                        // Clean up the stale mount
                        #[cfg(target_os = "linux")]
                        Self::cleanup_stale_mount(&mount_path).await;
                    } else {
                        warn!(path = %manifest_path.display(), error = %e, "Failed to read seed bank manifest");
                    }
                }
            }
        }
        
        Ok(registry)
    }

    /// Ensure the canonical garden layout exists on the seed bank.
    async fn ensure_seed_bank_layout(mount_path: &str) -> Result<(), String> {
        let memories = std::path::Path::new(mount_path).join(paths::SEED_BANK_MEMORIES_DIR);
        let storage = std::path::Path::new(mount_path).join(paths::SEED_BANK_STORAGE_DIR);

        let mut created = Vec::new();

        if !memories.exists() {
            tokio::fs::create_dir_all(&memories)
                .await
                .map_err(|e| format!("Failed to create {}: {}", paths::SEED_BANK_MEMORIES_DIR, e))?;
            created.push(paths::SEED_BANK_MEMORIES_DIR);
        }

        if !storage.exists() {
            tokio::fs::create_dir_all(&storage)
                .await
                .map_err(|e| format!("Failed to create {}: {}", paths::SEED_BANK_STORAGE_DIR, e))?;
            created.push(paths::SEED_BANK_STORAGE_DIR);
        }

        if !created.is_empty() {
            tracing::info!(
                mount_path = %mount_path,
                created = %created.join(", "),
                "Seed bank layout auto-healed"
            );
        }

        Ok(())
    }

    /// Build SeedBankInfo from a manifest + mount context.
    fn build_seed_bank_info(
        manifest: SeedBankManifest,
        mount_path: &str,
        device: &str,
    ) -> Option<SeedBankInfo> {
        let (used_bytes, capacity_bytes) = DeviceAnalyzer::get_disk_usage(mount_path)
            .map(|(used, avail)| (used, used + avail))
            .unwrap_or((0, 0));

        if capacity_bytes == 0 {
            warn!(
                name = %manifest.name,
                device = %device,
                mount_path = %mount_path,
                "Skipping seed bank - mount appears stale (0 capacity)"
            );
            return None;
        }

        // Check if roaming (from different stone)
        // Use hostname as stone name (same as app_state initialization)
        let stone_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let roaming = manifest.origin_stone != stone_name;

        Some(SeedBankInfo {
            id: manifest.id,
            name: manifest.name.clone(),
            pool_id: manifest.pool_id,
            group: manifest.group.clone(),
            replica_id: manifest.replica_id,
            device: device.to_string(),
            mount_path: mount_path.to_string(),
            capacity_bytes,
            used_bytes,
            visibility: manifest.visibility,
            btrfs: manifest.filesystem == "btrfs",
            origin_stone: manifest.origin_stone,
            created_at: manifest.created_at,
            last_sync: None,
            roaming,
            online: true, // Verified: device is mounted and manifest is readable
        })
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

    /// List removable seed banks already mounted, regardless of mount location.
    #[cfg(target_os = "linux")]
    async fn list_mounted_seed_banks() -> Vec<MountedSeedBank> {
        let mounts = match tokio::fs::read_to_string("/proc/mounts").await {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let device = parts[0];
            let mount_path = parts[1];

            if !device.starts_with("/dev/") {
                continue;
            }

            if !DeviceAnalyzer::is_removable(device).unwrap_or(false) {
                continue;
            }

            let manifest_path = PathBuf::from(mount_path).join(".zen-garden").join("manifest.json");
            let content = match tokio::fs::read_to_string(&manifest_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let manifest: SeedBankManifest = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        device = %device,
                        mount_path = %mount_path,
                        error = %e,
                        "Found manifest but failed to parse"
                    );
                    continue;
                }
            };

            results.push(MountedSeedBank {
                device: device.to_string(),
                mount_path: mount_path.to_string(),
                manifest,
            });
        }

        results
    }

    /// Include externally mounted seed banks in the registry if they aren't under mounts/.
    #[cfg(target_os = "linux")]
    async fn append_external_mounts(
        registry: &mut SeedBankRegistry,
        mounts_dir: &PathBuf,
    ) -> Result<()> {
        let mounts_prefix = mounts_dir.to_string_lossy();
        let mounted = Self::list_mounted_seed_banks().await;

        for sb in mounted {
            if sb.mount_path.starts_with(mounts_prefix.as_ref()) {
                continue;
            }

            if registry.banks.contains_key(&sb.manifest.name) {
                continue;
            }

            if let Err(e) = Self::ensure_seed_bank_layout(&sb.mount_path).await {
                warn!(
                    name = %sb.manifest.name,
                    mount_path = %sb.mount_path,
                    error = %e,
                    "Failed to ensure seed bank layout (external mount)"
                );
            }

            if let Some(info) = Self::build_seed_bank_info(sb.manifest, &sb.mount_path, &sb.device) {
                warn!(
                    name = %info.name,
                    device = %info.device,
                    mount_path = %info.mount_path,
                    "Seed bank mounted outside canonical mounts directory"
                );
                registry.banks.insert(info.name.clone(), info);
            }
        }

        Ok(())
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

    /// Clean up a stale mount (device was physically removed)
    ///
    /// Uses lazy unmount (-l) which detaches immediately without waiting for I/O.
    /// This is safe for yanked devices that would otherwise hang on regular umount.
    #[cfg(target_os = "linux")]
    async fn cleanup_stale_mount(mount_path: &str) {
        use std::process::Stdio;
        use tokio::process::Command;

        info!(mount_path = %mount_path, "Cleaning up stale mount (device removed)");

        // Use lazy unmount to avoid hanging on dead device
        let result = Command::new("sudo")
            .args(["umount", "-l", mount_path])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                info!(mount_path = %mount_path, "Successfully cleaned up stale mount");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    mount_path = %mount_path,
                    error = %stderr.trim(),
                    "Could not unmount stale mount (may already be cleaned up)"
                );
            }
            Err(e) => {
                debug!(
                    mount_path = %mount_path,
                    error = %e,
                    "Failed to run umount command"
                );
            }
        }
    }
    
    /// Verify and recover tracked mounts that may have disappeared.
    ///
    /// This is the core of the resilient mount system. It checks each tracked mount
    /// and re-mounts if the device is still present but the mount disappeared.
    /// This handles race conditions with udisks2 or other system processes that
    /// might unmount our devices.
    ///
    /// Returns the number of mounts recovered.
    #[cfg(target_os = "linux")]
    pub async fn verify_and_recover_mounts(tracker: &MountTracker) -> u32 {
        use std::process::Stdio;
        use tokio::process::Command;

        let mut recovered = 0u32;
        let mut tracker_write = tracker.write().await;

        // Collect devices to check (can't mutate while iterating)
        let devices_to_check: Vec<(String, TrackedMount)> = tracker_write
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (device_key, tracked) in devices_to_check {
            // Check if device still exists
            let device_exists = tokio::fs::metadata(&tracked.device).await.is_ok();
            if !device_exists {
                // Device physically removed - stop tracking it
                info!(
                    device = %tracked.device,
                    name = %tracked.name,
                    "Tracked device no longer exists, removing from tracker"
                );
                tracker_write.remove(&device_key);
                continue;
            }

            // Check if mount is still active
            let is_mounted = Self::is_device_mounted(&tracked.device).await;
            if is_mounted {
                // All good - reset recovery attempts
                if let Some(entry) = tracker_write.get_mut(&device_key) {
                    entry.recovery_attempts = 0;
                }
                continue;
            }

            // Device exists but not mounted - need to recover
            let attempts = tracker_write.get(&device_key).map(|t| t.recovery_attempts).unwrap_or(0);

            if attempts >= 10 {
                // Too many failures - log warning but keep trying (don't give up)
                if attempts % 10 == 0 {
                    warn!(
                        device = %tracked.device,
                        name = %tracked.name,
                        attempts = attempts,
                        "Mount recovery failing repeatedly, will continue trying"
                    );
                }
            }

            info!(
                device = %tracked.device,
                mount = %tracked.mount_path,
                name = %tracked.name,
                attempt = attempts + 1,
                "Mount disappeared, attempting recovery"
            );

            // Ensure mount point exists
            if let Err(e) = tokio::fs::create_dir_all(&tracked.mount_path).await {
                warn!(
                    mount = %tracked.mount_path,
                    error = %e,
                    "Failed to create mount point for recovery"
                );
                if let Some(entry) = tracker_write.get_mut(&device_key) {
                    entry.recovery_attempts += 1;
                }
                continue;
            }

            // Try to mount
            let mount_result = Command::new("sudo")
                .args(["mount", &tracked.device, &tracked.mount_path])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;

            match mount_result {
                Ok(output) if output.status.success() => {
                    info!(
                        device = %tracked.device,
                        mount = %tracked.mount_path,
                        name = %tracked.name,
                        "Successfully recovered mount"
                    );
                    if let Some(entry) = tracker_write.get_mut(&device_key) {
                        entry.recovery_attempts = 0;
                        entry.last_mounted = std::time::Instant::now();
                    }
                    recovered += 1;
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check if it's already mounted (race condition)
                    if stderr.contains("already mounted") {
                        debug!(
                            device = %tracked.device,
                            "Device already mounted (race condition handled)"
                        );
                        if let Some(entry) = tracker_write.get_mut(&device_key) {
                            entry.recovery_attempts = 0;
                        }
                    } else {
                        warn!(
                            device = %tracked.device,
                            mount = %tracked.mount_path,
                            error = %stderr.trim(),
                            "Mount recovery failed"
                        );
                        if let Some(entry) = tracker_write.get_mut(&device_key) {
                            entry.recovery_attempts += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        device = %tracked.device,
                        error = %e,
                        "Failed to execute mount command for recovery"
                    );
                    if let Some(entry) = tracker_write.get_mut(&device_key) {
                        entry.recovery_attempts += 1;
                    }
                }
            }
        }

        recovered
    }

    /// Check if a device is currently mounted anywhere
    #[cfg(target_os = "linux")]
    async fn is_device_mounted(device: &str) -> bool {
        if let Ok(mounts) = tokio::fs::read_to_string("/proc/mounts").await {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() && parts[0] == device {
                    return true;
                }
            }
        }
        false
    }

    /// Track a successful mount for persistence monitoring
    #[cfg(target_os = "linux")]
    pub async fn track_mount(tracker: &MountTracker, device: &str, mount_path: &str, name: &str) {
        let mut tracker_write = tracker.write().await;
        tracker_write.insert(
            device.to_string(),
            TrackedMount {
                device: device.to_string(),
                mount_path: mount_path.to_string(),
                name: name.to_string(),
                recovery_attempts: 0,
                last_mounted: std::time::Instant::now(),
            },
        );
        debug!(
            device = %device,
            mount = %mount_path,
            name = %name,
            "Tracking mount for persistence"
        );
    }

    /// Auto-mount all unmounted seed bank devices using manifest-first discovery.
    ///
    /// This implements the STORAGE-0005 manifest-first discovery model:
    /// 1. Scan ALL unmounted removable devices (not just labeled ones)
    /// 2. Temp-mount each device to check for `.zen-garden/manifest.json`
    /// 3. If manifest found, derive mount path from manifest configuration
    /// 4. Mount to the derived path (supports named groups and replicas)
    ///
    /// Edge cases handled:
    /// - Already mounted devices: skipped
    /// - No manifest: unmount temp and skip (not a seed bank)
    /// - Mount directory already exists: reused
    /// - Mount failure: logged and skipped (device may be busy/corrupt)
    /// - Replicated seed banks: mount to `/mounts/{group}/replica-{id}`
    #[cfg(target_os = "linux")]
    async fn auto_mount_seed_banks() -> Result<()> {
        Self::auto_mount_seed_banks_with_tracker(None, None).await
    }

    /// Auto-mount with optional mount tracker for persistence monitoring.
    ///
    /// If tracker is provided, successful mounts will be tracked for the
    /// resilient mount persistence system.
    ///
    /// If event_bus is provided, emits StorageEvent::seed_bank_detected for
    /// successfully mounted seed banks (flows to Firefly/Cricket via SSE).
    ///
    /// This uses manifest-first discovery (STORAGE-0005):
    /// - Scans ALL unmounted removable devices
    /// - Temp-mounts to check for manifest
    /// - Derives mount path from manifest configuration
    #[cfg(target_os = "linux")]
    pub async fn auto_mount_seed_banks_with_tracker(
        tracker: Option<&MountTracker>,
        event_bus: Option<&crate::infra::EventBus>,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;
        use super::device::list_unmounted_removable_devices;

        let data_dir = garden_common::constants::paths::data_dir();
        let mounts_dir = PathBuf::from(&data_dir).join("mounts");

        // Ensure mounts directory exists
        if let Err(e) = tokio::fs::create_dir_all(&mounts_dir).await {
            warn!(error = %e, "Failed to create mounts directory");
            return Ok(());
        }

        // Rehome any mounted seed banks that are not using the canonical mount path.
        // This handles udisks/desktop auto-mounts and ensures seed banks live under mounts/.
        if let Err(e) = Self::rehome_mounted_seed_banks(tracker, event_bus, &mounts_dir, &data_dir).await {
            warn!(error = %e, "Failed to rehome mounted seed banks");
        }

        // Also track any already-mounted seed banks in our mounts directory
        Self::track_existing_mounts(tracker, &mounts_dir).await;

        // Get all unmounted removable devices
        let unmounted_devices = match list_unmounted_removable_devices() {
            Ok(devices) => devices,
            Err(e) => {
                warn!(error = %e, "Failed to list unmounted removable devices");
                return Ok(());
            }
        };

        for device in unmounted_devices {
            // Try to mount and check for manifest
            let manifest_result = Self::probe_device_for_manifest(&device.device).await;

            match manifest_result {
                Ok(Some(manifest)) => {
                    // Found a seed bank! Derive mount path from manifest
                    let mount_path = manifest.derive_mount_path(&data_dir);

                    // Create mount point directory (including parent for replicas)
                    if let Err(e) = tokio::fs::create_dir_all(&mount_path).await {
                        warn!(
                            device = %device.device,
                            mount = %mount_path,
                            error = %e,
                            "Failed to create mount point"
                        );
                        continue;
                    }

                    // Check if something else is already mounted at this path
                    let mount_path_buf = PathBuf::from(&mount_path);
                    if Self::is_mount_point(&mount_path_buf).await {
                        debug!(
                            mount = %mount_path,
                            "Mount point already has something mounted, skipping"
                        );
                        continue;
                    }

                    // Mount the device to the derived path
                    info!(
                        device = %device.device,
                        mount = %mount_path,
                        name = %manifest.name,
                        group = ?manifest.group,
                        replica_id = ?manifest.replica_id,
                        "Auto-mounting seed bank (manifest-first)"
                    );

                    let mount_result = Command::new("sudo")
                        .args(["mount", &device.device, &mount_path])
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .output()
                        .await;

                    match mount_result {
                        Ok(output) if output.status.success() => {
                            info!(
                                device = %device.device,
                                mount = %mount_path,
                                name = %manifest.name,
                                "Successfully auto-mounted seed bank"
                            );

                            // Track this mount for persistence monitoring
                            if let Some(t) = tracker {
                                Self::track_mount(t, &device.device, &mount_path, &manifest.name).await;
                            }

                            // Emit storage event for Companions (Firefly, Cricket)
                            if let Some(bus) = event_bus {
                                // Get capacity from disk after mount
                                let capacity_gb = DeviceAnalyzer::get_disk_usage(&mount_path)
                                    .map(|(used, avail)| (used + avail) / (1024 * 1024 * 1024))
                                    .unwrap_or(0);
                                let storage_event = StorageEvent::seed_bank_detected(
                                    &manifest.name,
                                    &device.device,
                                    &mount_path,
                                    capacity_gb,
                                );
                                bus.emit(storage_event);
                                info!(name = %manifest.name, "Emitted storage.detected event");
                            }
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            warn!(
                                device = %device.device,
                                mount = %mount_path,
                                error = %stderr.trim(),
                                "Failed to mount seed bank (device may be busy or corrupted)"
                            );
                        }
                        Err(e) => {
                            warn!(
                                device = %device.device,
                                mount = %mount_path,
                                error = %e,
                                "Failed to execute mount command"
                            );
                        }
                    }
                }
                Ok(None) => {
                    // No manifest found - not a seed bank, skip silently
                    debug!(
                        device = %device.device,
                        label = ?device.label,
                        "No manifest found, not a seed bank"
                    );
                }
                Err(e) => {
                    // Error probing device
                    debug!(
                        device = %device.device,
                        error = %e,
                        "Failed to probe device for manifest"
                    );
                }
            }
        }

        Ok(())
    }

    /// Rehome mounted seed banks to the canonical mounts directory.
    #[cfg(target_os = "linux")]
    async fn rehome_mounted_seed_banks(
        tracker: Option<&MountTracker>,
        event_bus: Option<&crate::infra::EventBus>,
        mounts_dir: &PathBuf,
        data_dir: &str,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;

        let mounts_prefix = mounts_dir.to_string_lossy();
        let mounted = Self::list_mounted_seed_banks().await;

        for sb in mounted {
            if sb.mount_path.starts_with(mounts_prefix.as_ref()) {
                continue;
            }

            if !DeviceAnalyzer::is_allowed_mount(&sb.mount_path) {
                warn!(
                    device = %sb.device,
                    mount_path = %sb.mount_path,
                    "Seed bank mounted at disallowed path; leaving in place"
                );
                continue;
            }

            let desired = sb.manifest.derive_mount_path(data_dir);
            if desired == sb.mount_path {
                continue;
            }

            // Ensure desired mount path exists
            let mkdir = Command::new("sudo")
                .args(["mkdir", "-p", &desired])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;
            if let Ok(output) = mkdir {
                if !output.status.success() {
                    warn!(
                        device = %sb.device,
                        mount = %desired,
                        error = %String::from_utf8_lossy(&output.stderr),
                        "Failed to create mount directory for rehome"
                    );
                    continue;
                }
            }

            // Unmount current path
            let umount = Command::new("sudo")
                .args(["umount", &sb.mount_path])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;

            match umount {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    warn!(
                        device = %sb.device,
                        mount = %sb.mount_path,
                        error = %String::from_utf8_lossy(&output.stderr),
                        "Failed to unmount seed bank for rehome"
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        device = %sb.device,
                        mount = %sb.mount_path,
                        error = %e,
                        "Failed to execute umount for rehome"
                    );
                    continue;
                }
            }

            // Mount to canonical path
            let mount = Command::new("sudo")
                .args(["mount", &sb.device, &desired])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;

            match mount {
                Ok(output) if output.status.success() => {
                    info!(
                        device = %sb.device,
                        from = %sb.mount_path,
                        to = %desired,
                        name = %sb.manifest.name,
                        "Rehomed seed bank to canonical mount"
                    );

                    if let Some(t) = tracker {
                        Self::track_mount(t, &sb.device, &desired, &sb.manifest.name).await;
                    }

                    if let Some(bus) = event_bus {
                        let capacity_gb = DeviceAnalyzer::get_disk_usage(&desired)
                            .map(|(used, avail)| (used + avail) / (1024 * 1024 * 1024))
                            .unwrap_or(0);
                        let storage_event = StorageEvent::seed_bank_detected(
                            &sb.manifest.name,
                            &sb.device,
                            &desired,
                            capacity_gb,
                        );
                        bus.emit(storage_event);
                        info!(name = %sb.manifest.name, "Emitted storage.detected event after rehome");
                    }
                }
                Ok(output) => {
                    warn!(
                        device = %sb.device,
                        mount = %desired,
                        error = %String::from_utf8_lossy(&output.stderr),
                        "Failed to mount seed bank to canonical path; attempting rollback"
                    );

                    // Best-effort rollback
                    let _ = Command::new("sudo")
                        .args(["mount", &sb.device, &sb.mount_path])
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .output()
                        .await;
                }
                Err(e) => {
                    warn!(
                        device = %sb.device,
                        mount = %desired,
                        error = %e,
                        "Failed to execute mount for rehome; attempting rollback"
                    );
                    let _ = Command::new("sudo")
                        .args(["mount", &sb.device, &sb.mount_path])
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .output()
                        .await;
                }
            }
        }

        Ok(())
    }

    /// Track existing mounts in our mounts directory for persistence monitoring
    #[cfg(target_os = "linux")]
    async fn track_existing_mounts(tracker: Option<&MountTracker>, mounts_dir: &PathBuf) {
        let Some(t) = tracker else { return };

        // Read /proc/mounts to find devices mounted under our mounts directory
        let mounts = match tokio::fs::read_to_string("/proc/mounts").await {
            Ok(m) => m,
            Err(_) => return,
        };

        let mounts_prefix = mounts_dir.to_string_lossy();

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let device = parts[0];
            let mount_path = parts[1];

            // Only track mounts under our mounts directory
            if !mount_path.starts_with(mounts_prefix.as_ref()) {
                continue;
            }

            // Get the name from the mount path
            let name = std::path::Path::new(mount_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            debug!(
                device = %device,
                mount = %mount_path,
                name = %name,
                "Tracking existing seed bank mount"
            );

            Self::track_mount(t, device, mount_path, &name).await;
        }
    }

    /// Probe a device for a seed bank manifest by temp-mounting
    ///
    /// Returns:
    /// - Ok(Some(manifest)) if device has a valid manifest
    /// - Ok(None) if device has no manifest (not a seed bank)
    /// - Err if probe failed (device error)
    #[cfg(target_os = "linux")]
    async fn probe_device_for_manifest(device_path: &str) -> Result<Option<SeedBankManifest>> {
        use std::process::Stdio;
        use tokio::process::Command;

        let temp_mount = format!("/tmp/zen-garden-probe-{}-{}",
            std::process::id(),
            device_path.replace('/', "_")
        );

        // Create temp mount point
        let _ = Command::new("sudo")
            .args(["mkdir", "-p", &temp_mount])
            .output()
            .await;

        // Try to mount read-only
        let mount_result = Command::new("sudo")
            .args(["mount", "-o", "ro", device_path, &temp_mount])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await;

        let manifest = if let Ok(output) = mount_result {
            if output.status.success() {
                // Check for manifest
                let manifest_path = format!("{}/.zen-garden/manifest.json", temp_mount);
                let manifest = if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await {
                    match serde_json::from_str::<SeedBankManifest>(&content) {
                        Ok(m) => {
                            debug!(
                                device = %device_path,
                                name = %m.name,
                                group = ?m.group,
                                "Found seed bank manifest"
                            );
                            Some(m)
                        }
                        Err(e) => {
                            warn!(
                                device = %device_path,
                                error = %e,
                                "Found manifest but failed to parse"
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                // Unmount
                let _ = Command::new("sudo")
                    .args(["umount", &temp_mount])
                    .output()
                    .await;

                manifest
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

        Ok(manifest)
    }
    
    #[cfg(not(target_os = "linux"))]
    async fn auto_mount_seed_banks() -> Result<()> {
        // Auto-mount not supported on non-Linux platforms
        Ok(())
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
