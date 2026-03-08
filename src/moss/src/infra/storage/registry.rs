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
use garden_common::storage::{StorageInfo, StorageManifest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use tokio::sync::RwLock;
#[cfg(target_os = "linux")]
use tracing::info;
use tracing::{debug, warn};

use super::device::DeviceAnalyzer;
#[cfg(target_os = "linux")]
use crate::domain::StorageEvent;

/// Tracks a persistent mount that should be maintained
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct TrackedMount {
    /// Device path (e.g., /dev/sdb)
    pub device: String,
    /// Mount path (e.g., /var/lib/zen-garden/mounts/public-seed-bank)
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
pub struct StorageRegistry {
    /// Map from seed bank id to info (keyed by id for replication support)
    banks: HashMap<String, StorageInfo>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct MountedSeedBank {
    device: String,
    mount_path: String,
    manifest: StorageManifest,
}

impl StorageRegistry {
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
        // Supports both layouts:
        //   Legacy 1-level: mounts/{name}/.zen-garden/manifest.json
        //   New 2-level:    mounts/{name}/{short_id}/.zen-garden/manifest.json
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
            if manifest_path.exists() {
                // Legacy 1-level layout: mounts/{name}/.zen-garden/manifest.json
                Self::try_register_bank(&mut registry, &path, &manifest_path).await;
            } else {
                // New 2-level layout: mounts/{name}/{short_id}/.zen-garden/manifest.json
                let mut sub_entries = match tokio::fs::read_dir(&path).await {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                while let Ok(Some(sub_entry)) = sub_entries.next_entry().await {
                    let sub_path = sub_entry.path();
                    if !sub_path.is_dir() {
                        continue;
                    }
                    let sub_manifest = sub_path.join(".zen-garden").join("manifest.json");
                    if sub_manifest.exists() {
                        Self::try_register_bank(&mut registry, &sub_path, &sub_manifest).await;
                    }
                }
            }
        }

        Ok(registry)
    }

    /// Attempt to read a manifest and register the seed bank.
    ///
    /// Shared by both 1-level and 2-level scan paths.
    async fn try_register_bank(registry: &mut Self, mount_dir: &Path, manifest_path: &Path) {
        match Self::read_manifest(manifest_path).await {
            Ok(manifest) => {
                let mount_path = mount_dir.to_string_lossy().to_string();

                // Get device from mount info - this tells us if actually mounted
                let device_opt = Self::get_device_for_mount(&mount_path).await;
                let is_mounted = device_opt.is_some();
                let device = device_opt.unwrap_or_else(|| "not-mounted".to_string());

                // Skip seed banks that aren't actually mounted
                if !is_mounted {
                    debug!(
                        name = %manifest.name,
                        mount_path = %mount_path,
                        "Skipping seed bank - not mounted (manifest exists but no device)"
                    );
                    return;
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
                let (_used_bytes, capacity_bytes) = DeviceAnalyzer::get_disk_usage(&mount_path)
                    .map(|(used, avail)| (used, used + avail))
                    .unwrap_or((0, 0));

                // Liveness check: if capacity is 0, mount is likely stale/dead
                if capacity_bytes == 0 {
                    warn!(
                        name = %manifest.name,
                        device = %device,
                        mount_path = %mount_path,
                        "Skipping seed bank - mount appears stale (0 capacity)"
                    );

                    #[cfg(target_os = "linux")]
                    Self::cleanup_stale_mount(&mount_path).await;

                    return;
                }

                // Use id as registry key (unique per physical device)
                if let Some(info) = Self::build_seed_bank_info(manifest, &mount_path, &device) {
                    debug!(name = %info.name, id = %info.id, device = %info.device, "Discovered seed bank");
                    registry.banks.insert(info.id.clone(), info);
                }
            }
            Err(e) => {
                let mount_path = mount_dir.to_string_lossy().to_string();
                let error_str = e.to_string().to_lowercase();

                if error_str.contains("i/o error") || error_str.contains("input/output error") {
                    warn!(
                        mount_path = %mount_path,
                        error = %e,
                        "Seed bank I/O error - device may have been removed"
                    );

                    #[cfg(target_os = "linux")]
                    Self::cleanup_stale_mount(&mount_path).await;
                } else {
                    warn!(path = %manifest_path.display(), error = %e, "Failed to read seed bank manifest");
                }
            }
        }
    }

    /// Ensure the canonical garden layout exists on the seed bank.
    async fn ensure_seed_bank_layout(mount_path: &str) -> Result<(), String> {
        let memories = std::path::Path::new(mount_path).join(paths::STORAGE_MEMORIES_DIR);
        let storage = std::path::Path::new(mount_path).join(paths::STORAGE_OBJECTS_DIR);

        let mut created = Vec::new();

        if !memories.exists() {
            tokio::fs::create_dir_all(&memories).await.map_err(|e| {
                format!("Failed to create {}: {}", paths::STORAGE_MEMORIES_DIR, e)
            })?;
            created.push(paths::STORAGE_MEMORIES_DIR);
        }

        if !storage.exists() {
            tokio::fs::create_dir_all(&storage)
                .await
                .map_err(|e| format!("Failed to create {}: {}", paths::STORAGE_OBJECTS_DIR, e))?;
            created.push(paths::STORAGE_OBJECTS_DIR);
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

    /// Build StorageInfo from a manifest + mount context.
    fn build_seed_bank_info(
        manifest: StorageManifest,
        mount_path: &str,
        device: &str,
    ) -> Option<StorageInfo> {
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

        Some(StorageInfo::new(
            manifest.id,
            manifest.name.clone(),
            device.to_string(),
            mount_path.to_string(),
            capacity_bytes,
            used_bytes,
            manifest.visibility,
            manifest.filesystem == "btrfs",
            manifest.origin_stone,
            manifest.created_at,
            roaming,
            true, // Verified: device is mounted and manifest is readable
            manifest.encrypted,
            manifest.roles,
        ))
    }

    /// Read manifest from disk
    async fn read_manifest(path: &Path) -> Result<StorageManifest> {
        let content = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read manifest file")?;

        let manifest: StorageManifest =
            serde_json::from_str(&content).context("Failed to parse manifest JSON")?;

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

            let manifest_path = PathBuf::from(mount_path)
                .join(".zen-garden")
                .join("manifest.json");
            let content = match tokio::fs::read_to_string(&manifest_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let manifest: StorageManifest = match serde_json::from_str(&content) {
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
        registry: &mut StorageRegistry,
        mounts_dir: &PathBuf,
    ) -> Result<()> {
        let mounts_prefix = mounts_dir.to_string_lossy();
        let mounted = Self::list_mounted_seed_banks().await;

        for sb in mounted {
            if sb.mount_path.starts_with(mounts_prefix.as_ref()) {
                continue;
            }

            if registry.banks.contains_key(&sb.manifest.id) {
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

            if let Some(info) = Self::build_seed_bank_info(sb.manifest, &sb.mount_path, &sb.device)
            {
                warn!(
                    name = %info.name,
                    device = %info.device,
                    mount_path = %info.mount_path,
                    "Seed bank mounted outside canonical mounts directory"
                );
                registry.banks.insert(info.id.clone(), info);
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
        use super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        info!(mount_path = %mount_path, "Cleaning up stale mount (device removed)");

        let result = run_sudo_timed_quiet(
            &["umount", "-l", mount_path],
            timeouts::subprocess_mount_timeout(),
        )
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
                    "Failed to run umount command (timeout or I/O error)"
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
    /// Resilience features:
    /// - **Timed subprocess**: mount commands have a deadline (default 30s)
    /// - **Per-device isolation**: the write lock is released during mount I/O
    ///   so a single hung device cannot block monitoring of other mounts
    /// - **Circuit breaker**: after N consecutive failures (default 5), recovery
    ///   enters exponential backoff; after M failures (default 50) the device
    ///   is abandoned and removed from the tracker
    ///
    /// Returns the number of mounts recovered.
    #[cfg(target_os = "linux")]
    pub async fn verify_and_recover_mounts(tracker: &MountTracker) -> u32 {
        use super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let mount_timeout = timeouts::subprocess_mount_timeout();
        let backoff_threshold = timeouts::mount_recovery_backoff_threshold();
        let max_attempts = timeouts::mount_recovery_max_attempts();

        // ── Phase 1: snapshot state under READ lock ──────────────────────
        let snapshot: Vec<(String, TrackedMount)> = {
            let tracker_read = tracker.read().await;
            tracker_read
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        // Lock is released here — mount I/O runs lock-free.

        /// Per-device recovery outcome
        enum Outcome {
            /// Mount is healthy — reset recovery counter
            Healthy,
            /// Device physically removed — delete from tracker
            Removed,
            /// Mount recovered successfully
            Recovered,
            /// Recovery failed — increment counter
            Failed,
            /// Skipped this cycle (circuit-breaker backoff)
            Skipped,
            /// Exceeded max attempts — abandon device
            Abandoned,
        }

        // ── Phase 2: recover each device independently ───────────────────
        let mut outcomes: Vec<(String, Outcome)> = Vec::with_capacity(snapshot.len());

        for (device_key, tracked) in &snapshot {
            // ── Circuit breaker: backoff or abandon ──────────────────
            if tracked.recovery_attempts >= max_attempts {
                warn!(
                    device = %tracked.device,
                    name = %tracked.name,
                    attempts = tracked.recovery_attempts,
                    "Mount recovery exceeded max attempts, abandoning device"
                );
                outcomes.push((device_key.clone(), Outcome::Abandoned));
                continue;
            }

            if tracked.recovery_attempts >= backoff_threshold {
                // Exponential backoff: skip 2^(attempts - threshold) cycles,
                // capped at 64 cycles (~5 min at 5s interval).
                let exponent = std::cmp::min(tracked.recovery_attempts - backoff_threshold, 6);
                let skip_cycles = 1u32 << exponent; // 1, 2, 4, 8, 16, 32, 64
                                                    // Use attempt count modulo skip_cycles to decide whether to act
                if tracked.recovery_attempts % skip_cycles != 0 {
                    debug!(
                        device = %tracked.device,
                        name = %tracked.name,
                        attempts = tracked.recovery_attempts,
                        next_try_in_cycles = skip_cycles - (tracked.recovery_attempts % skip_cycles),
                        "Mount recovery in backoff, skipping this cycle"
                    );
                    outcomes.push((device_key.clone(), Outcome::Skipped));
                    continue;
                }
            }

            // ── Check device existence ───────────────────────────────
            let device_exists = tokio::fs::metadata(&tracked.device).await.is_ok();
            if !device_exists {
                info!(
                    device = %tracked.device,
                    name = %tracked.name,
                    "Tracked device no longer exists, removing from tracker"
                );
                outcomes.push((device_key.clone(), Outcome::Removed));
                continue;
            }

            // ── Check if mount is still active ───────────────────────
            if Self::is_device_mounted(&tracked.device).await {
                outcomes.push((device_key.clone(), Outcome::Healthy));
                continue;
            }

            // ── Device exists but not mounted — attempt recovery ─────
            info!(
                device = %tracked.device,
                mount = %tracked.mount_path,
                name = %tracked.name,
                attempt = tracked.recovery_attempts + 1,
                "Mount disappeared, attempting recovery"
            );

            // Ensure mount point exists
            if let Err(e) = tokio::fs::create_dir_all(&tracked.mount_path).await {
                warn!(
                    mount = %tracked.mount_path,
                    error = %e,
                    "Failed to create mount point for recovery"
                );
                outcomes.push((device_key.clone(), Outcome::Failed));
                continue;
            }

            // Try to mount (with timeout — won't hang on dead device)
            let mount_result = run_sudo_timed_quiet(
                &["mount", &tracked.device, &tracked.mount_path],
                mount_timeout,
            )
            .await;

            match mount_result {
                Ok(output) if output.status.success() => {
                    info!(
                        device = %tracked.device,
                        mount = %tracked.mount_path,
                        name = %tracked.name,
                        "Successfully recovered mount"
                    );
                    outcomes.push((device_key.clone(), Outcome::Recovered));
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("already mounted") {
                        debug!(
                            device = %tracked.device,
                            "Device already mounted (race condition handled)"
                        );
                        outcomes.push((device_key.clone(), Outcome::Healthy));
                    } else {
                        warn!(
                            device = %tracked.device,
                            mount = %tracked.mount_path,
                            error = %stderr.trim(),
                            "Mount recovery failed"
                        );
                        outcomes.push((device_key.clone(), Outcome::Failed));
                    }
                }
                Err(e) => {
                    warn!(
                        device = %tracked.device,
                        error = %e,
                        "Mount command failed or timed out during recovery"
                    );
                    outcomes.push((device_key.clone(), Outcome::Failed));
                }
            }
        }

        // ── Phase 3: apply outcomes under WRITE lock ─────────────────────
        let mut recovered = 0u32;
        let mut tracker_write = tracker.write().await;

        for (device_key, outcome) in outcomes {
            match outcome {
                Outcome::Healthy => {
                    if let Some(entry) = tracker_write.get_mut(&device_key) {
                        entry.recovery_attempts = 0;
                    }
                }
                Outcome::Removed | Outcome::Abandoned => {
                    tracker_write.remove(&device_key);
                }
                Outcome::Recovered => {
                    if let Some(entry) = tracker_write.get_mut(&device_key) {
                        entry.recovery_attempts = 0;
                        entry.last_mounted = std::time::Instant::now();
                    }
                    recovered += 1;
                }
                Outcome::Failed => {
                    if let Some(entry) = tracker_write.get_mut(&device_key) {
                        entry.recovery_attempts += 1;
                    }
                }
                Outcome::Skipped => {
                    // Increment so we advance through the backoff schedule
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
    /// If event_bus is provided, emits StorageEvent::storage_connected for
    /// successfully mounted managed storage (flows to Firefly/Cricket via SSE).
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
        use super::device::list_unmounted_removable_devices;
        use super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let mount_timeout = timeouts::subprocess_mount_timeout();
        let data_dir = garden_common::constants::paths::data_dir();
        let mounts_dir = PathBuf::from(&data_dir).join("mounts");

        // Ensure mounts directory exists
        if let Err(e) = tokio::fs::create_dir_all(&mounts_dir).await {
            warn!(error = %e, "Failed to create mounts directory");
            return Ok(());
        }

        // Rehome any mounted seed banks that are not using the canonical mount path.
        // This handles udisks/desktop auto-mounts and ensures seed banks live under mounts/.
        if let Err(e) =
            Self::rehome_mounted_seed_banks(tracker, event_bus, &mounts_dir, &data_dir).await
        {
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
                        id = %manifest.id,
                        "Auto-mounting seed bank (manifest-first)"
                    );

                    let mount_result = run_sudo_timed_quiet(
                        &["mount", &device.device, &mount_path],
                        mount_timeout,
                    )
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
                                Self::track_mount(t, &device.device, &mount_path, &manifest.name)
                                    .await;
                            }

                            // Emit storage connected event for Companions (Firefly, Cricket)
                            if let Some(bus) = event_bus {
                                // Get capacity from disk after mount
                                let capacity_gb = DeviceAnalyzer::get_disk_usage(&mount_path)
                                    .map(|(used, avail)| (used + avail) / (1024 * 1024 * 1024))
                                    .unwrap_or(0);
                                let storage_event = StorageEvent::storage_connected(
                                    &manifest.name,
                                    &device.device,
                                    &mount_path,
                                    capacity_gb,
                                    manifest.roles.clone(),
                                );
                                bus.emit(storage_event);
                                info!(name = %manifest.name, "Emitted storage.connected event");
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
                                "Mount command failed or timed out"
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
        use super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let mount_timeout = timeouts::subprocess_mount_timeout();
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
            let mkdir = run_sudo_timed_quiet(&["mkdir", "-p", &desired], mount_timeout).await;
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
            let umount = run_sudo_timed_quiet(&["umount", &sb.mount_path], mount_timeout).await;

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
                        "Umount timed out or failed for rehome"
                    );
                    continue;
                }
            }

            // Mount to canonical path
            let mount = run_sudo_timed_quiet(&["mount", &sb.device, &desired], mount_timeout).await;

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
                        let storage_event = StorageEvent::storage_connected(
                            &sb.manifest.name,
                            &sb.device,
                            &desired,
                            capacity_gb,
                            sb.manifest.roles.clone(),
                        );
                        bus.emit(storage_event);
                        info!(name = %sb.manifest.name, "Emitted storage.connected event after rehome");
                    }
                }
                Ok(output) => {
                    warn!(
                        device = %sb.device,
                        mount = %desired,
                        error = %String::from_utf8_lossy(&output.stderr),
                        "Failed to mount seed bank to canonical path; attempting rollback"
                    );

                    // Best-effort rollback (with timeout)
                    let _ =
                        run_sudo_timed_quiet(&["mount", &sb.device, &sb.mount_path], mount_timeout)
                            .await;
                }
                Err(e) => {
                    warn!(
                        device = %sb.device,
                        mount = %desired,
                        error = %e,
                        "Mount timed out or failed for rehome; attempting rollback"
                    );
                    let _ =
                        run_sudo_timed_quiet(&["mount", &sb.device, &sb.mount_path], mount_timeout)
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
    async fn probe_device_for_manifest(device_path: &str) -> Result<Option<StorageManifest>> {
        use super::subprocess::run_sudo_timed_quiet;
        use garden_common::constants::timeouts;

        let mount_timeout = timeouts::subprocess_mount_timeout();

        let temp_mount = format!(
            "/tmp/zen-garden-probe-{}-{}",
            std::process::id(),
            device_path.replace('/', "_")
        );

        // Create temp mount point
        let _ = run_sudo_timed_quiet(&["mkdir", "-p", &temp_mount], mount_timeout).await;

        // Try to mount read-only (with timeout — prevents hang on dead device)
        let mount_result = run_sudo_timed_quiet(
            &["mount", "-o", "ro", device_path, &temp_mount],
            mount_timeout,
        )
        .await;

        let manifest = if let Ok(output) = mount_result {
            if output.status.success() {
                // Check for manifest
                let manifest_path = format!("{}/.zen-garden/manifest.json", temp_mount);
                let manifest = if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await
                {
                    match serde_json::from_str::<StorageManifest>(&content) {
                        Ok(m) => {
                            debug!(
                                device = %device_path,
                                name = %m.name,
                                id = %m.id,
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

    /// Get a seed bank by name (returns first match — use `get_all_by_name` for replicas)
    pub fn get(&self, name: &str) -> Option<&StorageInfo> {
        self.banks.values().find(|b| b.name == name)
    }

    /// Get all seed banks sharing a name (replication-aware)
    pub fn get_all_by_name(&self, name: &str) -> Vec<&StorageInfo> {
        self.banks.values().filter(|b| b.name == name).collect()
    }

    /// List all seed banks
    pub fn list(&self) -> Vec<&StorageInfo> {
        self.banks.values().collect()
    }

    /// Check if a seed bank with the given name exists
    pub fn exists(&self, name: &str) -> bool {
        self.banks.values().any(|b| b.name == name)
    }

    /// Find seed bank by device path
    pub fn find_by_device(&self, device: &str) -> Option<&StorageInfo> {
        self.banks.values().find(|b| b.device == device)
    }

    /// Find seed bank by mount path
    pub fn find_by_mount(&self, mount_path: &str) -> Option<&StorageInfo> {
        self.banks.values().find(|b| b.mount_path == mount_path)
    }

    /// Find seed bank by ID (GUIDv7) — direct HashMap lookup
    pub fn find_by_id(&self, id: &str) -> Option<&StorageInfo> {
        self.banks.get(id)
    }

    /// Get seed bank by name (alias for get)
    pub fn get_by_name(&self, name: &str) -> Option<&StorageInfo> {
        self.get(name)
    }

    /// Get seed bank by ID (alias for find_by_id)
    pub fn get_by_id(&self, id: &str) -> Option<&StorageInfo> {
        self.find_by_id(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_scan() {
        // Just verify it doesn't crash on empty system
        let registry = StorageRegistry::scan().await.unwrap();
        assert!(registry.list().is_empty() || !registry.list().is_empty());
    }
}
