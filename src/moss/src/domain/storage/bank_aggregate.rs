//! Bank aggregate — the user-facing named storage container (ARCH-0025).
//!
//! A **Bank** groups volumes across stones under a single FQN ("personal",
//! "media"). It is a view aggregate in VIII-a: derived from the volume
//! collection at query time rather than maintained as a separate persistent
//! entity. Volumes persist the bank identity (`replica_set_id`,
//! `replica_set_name`) in their on-disk manifests.
//!
//! ## Commands
//!
//! - `rename` — rename all volumes in this bank
//! - `set_roles` — set composable roles on all local volumes
//! - `set_visibility` — set visibility on all local volumes
//! - `pin` / `unpin` — claim or release Primary role
//!
//! ## Queries
//!
//! - `local_banks()` — list distinct banks with a volume on this stone
//! - `by_name(name)` — single bank lookup
//! - `primary_volume(name)` — which volume accepts writes
//! - `volumes_for_bank(name)` — all local volumes in a bank

use std::collections::HashMap;
use std::path::PathBuf;

use garden_common::storage::{StorageChanged, StorageInfo, StorageRole, StorageVisibility};

use super::ports::ManagementStoreOps;
use super::volume::Volume;
use super::Volumes;

// ============================================================================
// Bank — view projection from the volume collection
// ============================================================================

/// A bank is a named storage group. This struct is a read-only view
/// projected from the volume collection, not a separately-persisted entity.
#[derive(Debug, Clone)]
pub struct Bank {
    /// Replica set ID (GUIDv7). Shared by all volumes in this bank.
    pub id: String,
    /// User-facing display name (the FQN: "personal", "media").
    pub name: String,
    /// Composable roles (e.g., "seed-bank", "storage").
    pub roles: Vec<String>,
    /// Visibility (Open, Closed, ReadOnly).
    pub visibility: StorageVisibility,
    /// Number of local volumes in this bank.
    pub local_volume_count: usize,
    /// Primary volume device path, if one exists locally.
    pub primary_device: Option<String>,
    /// Total capacity across local volumes.
    pub capacity_bytes: u64,
    /// Total used space across local volumes.
    pub used_bytes: u64,
    /// Whether any local volume is encrypted.
    pub encrypted: bool,
    /// Mount path of the first online volume (for store construction).
    pub mount_path: Option<PathBuf>,
}

impl Bank {
    /// Whether this bank has a locally-pinned Primary volume.
    pub fn has_local_primary(&self) -> bool {
        self.primary_device.is_some()
    }
}

// ============================================================================
// Queries — read-only projections from the volume collection
// ============================================================================

/// List all distinct banks that have at least one managed, online volume
/// on this stone. Groups volumes by `replica_set_id`.
pub async fn local_banks(volumes: &Volumes) -> Vec<Bank> {
    let map = volumes.read().await;
    let mut banks: HashMap<String, Bank> = HashMap::new();

    for vol in map.values() {
        if !vol.is_managed() || !vol.is_online() {
            continue;
        }
        let mgmt = vol.management().unwrap(); // safe: is_managed() checked
        let rs_id = &mgmt.replica_set_id;

        let bank = banks.entry(rs_id.clone()).or_insert_with(|| Bank {
            id: rs_id.clone(),
            name: mgmt.display_name().to_string(),
            roles: mgmt.roles.clone(),
            visibility: mgmt.visibility,
            local_volume_count: 0,
            primary_device: None,
            capacity_bytes: 0,
            used_bytes: 0,
            encrypted: mgmt.encrypted,
            mount_path: None,
        });

        bank.local_volume_count += 1;
        bank.capacity_bytes += vol.capacity_bytes();
        bank.used_bytes += vol.used_bytes();

        if mgmt.role == StorageRole::Primary && bank.primary_device.is_none() {
            bank.primary_device = Some(vol.path().to_string());
        }
        if bank.mount_path.is_none() {
            bank.mount_path = Some(vol.mount_path().clone());
        }
        // Merge: any encrypted volume → bank is encrypted
        if mgmt.encrypted {
            bank.encrypted = true;
        }
    }

    banks.into_values().collect()
}

/// Find a bank by its user-facing name. Returns `None` if no managed,
/// online volume on this stone matches.
pub async fn by_name(name: &str, volumes: &Volumes) -> Option<Bank> {
    let banks = local_banks(volumes).await;
    banks.into_iter().find(|b| b.name == name)
}

/// Find the primary volume for a bank by name.
/// Returns the Volume clone if a Primary-role volume exists locally.
pub async fn primary_volume(name: &str, volumes: &Volumes) -> Option<Volume> {
    let map = volumes.read().await;
    map.values()
        .find(|v| {
            v.is_managed()
                && v.is_online()
                && v.management()
                    .is_some_and(|m| m.display_name() == name && m.role == StorageRole::Primary)
        })
        .cloned()
}

/// All local volumes belonging to a bank (by replica set name).
pub async fn volumes_for_bank(name: &str, volumes: &Volumes) -> Vec<Volume> {
    let map = volumes.read().await;
    map.values()
        .filter(|v| {
            v.is_managed()
                && v.management()
                    .is_some_and(|m| m.display_name() == name)
        })
        .cloned()
        .collect()
}

// ============================================================================
// Commands — bank-level mutations across all member volumes
// ============================================================================

/// Rename a bank: updates `replica_set_name` on all local volumes in the
/// replica set. Returns (replica_set_id, domain events) for the caller
/// to emit and persist.
///
/// Does NOT persist to disk manifests — the caller must do that (infra
/// concern). Returns the mount paths of affected volumes for manifest
/// persistence.
pub async fn rename(
    old_name: &str,
    new_name: &str,
    volumes: &Volumes,
) -> Result<RenameResult, BankError> {
    if new_name.is_empty() {
        return Err(BankError::InvalidName("name cannot be empty".into()));
    }

    let mut map = volumes.write().await;

    // Find all volumes in this bank
    let device_keys: Vec<String> = map
        .iter()
        .filter(|(_, v)| {
            v.is_managed()
                && v.management()
                    .is_some_and(|m| m.display_name() == old_name)
        })
        .map(|(k, _)| k.clone())
        .collect();

    if device_keys.is_empty() {
        return Err(BankError::NotFound(old_name.to_string()));
    }

    let mut events = Vec::new();
    let mut mount_paths = Vec::new();
    let mut replica_set_id = String::new();

    for key in &device_keys {
        if let Some(vol) = map.get_mut(key) {
            if let Some(mgmt) = vol.management()
                && replica_set_id.is_empty()
            {
                replica_set_id = mgmt.replica_set_id.clone();
            }
            mount_paths.push(vol.mount_path().to_string_lossy().to_string());
            events.extend(vol.rename(new_name.to_string()));
        }
    }

    Ok(RenameResult {
        replica_set_id,
        mount_paths,
        events,
    })
}

/// Set composable roles on all local volumes in a bank.
pub async fn set_roles(
    name: &str,
    roles: Vec<String>,
    volumes: &Volumes,
) -> Result<Vec<StorageChanged>, BankError> {
    let mut map = volumes.write().await;
    let mut events = Vec::new();
    let mut found = false;

    for vol in map.values_mut() {
        let matches = vol
            .management()
            .is_some_and(|m| m.display_name() == name);
        if matches {
            found = true;
            events.extend(vol.set_roles(roles.clone()));
        }
    }

    if !found {
        return Err(BankError::NotFound(name.to_string()));
    }
    Ok(events)
}

/// Set visibility on all local volumes in a bank.
pub async fn set_visibility(
    name: &str,
    visibility: StorageVisibility,
    volumes: &Volumes,
) -> Result<Vec<StorageChanged>, BankError> {
    let mut map = volumes.write().await;
    let mut events = Vec::new();
    let mut found = false;

    for vol in map.values_mut() {
        let matches = vol
            .management()
            .is_some_and(|m| m.display_name() == name);
        if matches {
            found = true;
            events.extend(vol.set_visibility(visibility));
        }
    }

    if !found {
        return Err(BankError::NotFound(name.to_string()));
    }
    Ok(events)
}

/// Pin a volume as Primary in a bank. Pins the first online volume found.
pub async fn pin<S: ManagementStoreOps>(
    name: &str,
    volumes: &Volumes,
    make_store: impl Fn(PathBuf) -> std::sync::Arc<S>,
) -> Result<Vec<StorageChanged>, BankError> {
    let mut map = volumes.write().await;

    let vol = map
        .values_mut()
        .find(|v| {
            v.is_managed()
                && v.is_online()
                && v.management()
                    .is_some_and(|m| m.display_name() == name)
        })
        .ok_or_else(|| BankError::NotFound(name.to_string()))?;

    let store = make_store(vol.mount_path().clone());
    vol.pin(store.as_ref())
        .await
        .map_err(|e| BankError::PinFailed(e.to_string()))
}

/// Unpin a volume in a bank (release the Primary claim).
pub async fn unpin<S: ManagementStoreOps>(
    name: &str,
    volumes: &Volumes,
    make_store: impl Fn(PathBuf) -> std::sync::Arc<S>,
) -> Result<Vec<StorageChanged>, BankError> {
    let mut map = volumes.write().await;

    let vol = map
        .values_mut()
        .find(|v| {
            v.is_managed()
                && v.management()
                    .is_some_and(|m| m.display_name() == name && m.pin.is_some())
        })
        .ok_or_else(|| BankError::NotFound(name.to_string()))?;

    let store = make_store(vol.mount_path().clone());
    vol.unpin(store.as_ref())
        .await
        .map_err(|e| BankError::UnpinFailed(e.to_string()))
}

/// Release (unmount) all volumes in a bank.
/// Returns events for all released volumes.
pub async fn release(
    name: &str,
    volumes: &Volumes,
) -> Result<(Vec<StorageChanged>, Vec<String>), BankError> {
    let mut map = volumes.write().await;
    let mut events = Vec::new();
    let mut mount_paths = Vec::new();
    let mut found = false;

    for vol in map.values_mut() {
        let matches = vol
            .management()
            .is_some_and(|m| m.display_name() == name);
        if matches {
            found = true;
            mount_paths.push(vol.mount_path().to_string_lossy().to_string());
            events.extend(vol.release());
        }
    }

    if !found {
        return Err(BankError::NotFound(name.to_string()));
    }
    Ok((events, mount_paths))
}

/// Project a bank-level `StorageInfo` for each local bank.
/// Used by API handlers for the list/get endpoints.
pub async fn bank_infos(volumes: &Volumes) -> Vec<StorageInfo> {
    let map = volumes.read().await;
    map.values().filter_map(|v| v.to_storage_info()).collect()
}

// ============================================================================
// Result and error types
// ============================================================================

/// Result of a bank rename operation.
pub struct RenameResult {
    /// The replica set ID of the renamed bank.
    pub replica_set_id: String,
    /// Mount paths of affected volumes (for manifest persistence).
    pub mount_paths: Vec<String>,
    /// Domain events to emit.
    pub events: Vec<StorageChanged>,
}

/// Bank-level error.
#[derive(Debug)]
pub enum BankError {
    /// No bank with this name exists locally.
    NotFound(String),
    /// Name validation failed.
    InvalidName(String),
    /// Pin operation failed.
    PinFailed(String),
    /// Unpin operation failed.
    UnpinFailed(String),
}

impl std::fmt::Display for BankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(name) => write!(f, "bank '{}' not found", name),
            Self::InvalidName(reason) => write!(f, "invalid bank name: {}", reason),
            Self::PinFailed(reason) => write!(f, "pin failed: {}", reason),
            Self::UnpinFailed(reason) => write!(f, "unpin failed: {}", reason),
        }
    }
}

impl std::error::Error for BankError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::storage::volume::{Management, PinState, VolumeState};
    use crate::domain::storage::new_volumes;
    use garden_common::storage::{StorageRole, StorageVisibility};

    fn make_managed_volume(
        device: &str,
        mount: &str,
        bank_name: &str,
        rs_id: &str,
        role: StorageRole,
    ) -> Volume {
        Volume::for_test(
            device,
            PathBuf::from(mount),
            None,
            100_000_000_000,
            10_000_000_000,
            true,
            VolumeState::Online,
            Some(Management {
                id: format!("vol-{}", device),
                short_id: "vol-xxxx".to_string(),
                name: format!("vol-{}", device),
                replica_set_id: rs_id.to_string(),
                replica_set_name: bank_name.to_string(),
                replica_set_name_updated_at: None,
                encrypted: false,
                roaming: false,
                origin_stone: "stone-test".to_string(),
                created_at: chrono::Utc::now(),
                visibility: StorageVisibility::Open,
                roles: vec!["seed-bank".to_string()],
                role,
                pin: None,
            }),
        )
    }

    #[tokio::test]
    async fn local_banks_groups_by_replica_set() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "personal", "rs-1", StorageRole::Primary),
            );
            map.insert(
                "/dev/sdb1".into(),
                make_managed_volume("/dev/sdb1", "/mnt/b", "personal", "rs-1", StorageRole::Dormant),
            );
            map.insert(
                "/dev/sdc1".into(),
                make_managed_volume("/dev/sdc1", "/mnt/c", "media", "rs-2", StorageRole::Primary),
            );
        }

        let banks = local_banks(&volumes).await;
        assert_eq!(banks.len(), 2);

        let personal = banks.iter().find(|b| b.name == "personal").unwrap();
        assert_eq!(personal.local_volume_count, 2);
        assert!(personal.has_local_primary());
        assert_eq!(personal.capacity_bytes, 200_000_000_000);

        let media = banks.iter().find(|b| b.name == "media").unwrap();
        assert_eq!(media.local_volume_count, 1);
        assert!(media.has_local_primary());
    }

    #[tokio::test]
    async fn by_name_returns_matching_bank() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
        }

        assert!(by_name("photos", &volumes).await.is_some());
        assert!(by_name("nonexistent", &volumes).await.is_none());
    }

    #[tokio::test]
    async fn primary_volume_finds_primary() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Dormant),
            );
            map.insert(
                "/dev/sdb1".into(),
                make_managed_volume("/dev/sdb1", "/mnt/b", "photos", "rs-1", StorageRole::Primary),
            );
        }

        let pv = primary_volume("photos", &volumes).await.unwrap();
        assert_eq!(pv.path(), "/dev/sdb1");

        assert!(primary_volume("nonexistent", &volumes).await.is_none());
    }

    #[tokio::test]
    async fn volumes_for_bank_returns_all() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
            map.insert(
                "/dev/sdb1".into(),
                make_managed_volume("/dev/sdb1", "/mnt/b", "photos", "rs-1", StorageRole::Dormant),
            );
            map.insert(
                "/dev/sdc1".into(),
                make_managed_volume("/dev/sdc1", "/mnt/c", "media", "rs-2", StorageRole::Primary),
            );
        }

        let vols = volumes_for_bank("photos", &volumes).await;
        assert_eq!(vols.len(), 2);

        let vols = volumes_for_bank("media", &volumes).await;
        assert_eq!(vols.len(), 1);
    }

    #[tokio::test]
    async fn rename_updates_all_volumes() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
            map.insert(
                "/dev/sdb1".into(),
                make_managed_volume("/dev/sdb1", "/mnt/b", "photos", "rs-1", StorageRole::Dormant),
            );
        }

        let result = rename("photos", "memories", &volumes).await.unwrap();
        assert_eq!(result.replica_set_id, "rs-1");
        assert_eq!(result.mount_paths.len(), 2);
        assert!(!result.events.is_empty());

        // Verify all volumes are renamed
        let bank = by_name("memories", &volumes).await;
        assert!(bank.is_some());
        assert!(by_name("photos", &volumes).await.is_none());
    }

    #[tokio::test]
    async fn rename_not_found() {
        let volumes = new_volumes();
        let result = rename("nonexistent", "new", &volumes).await;
        assert!(matches!(result, Err(BankError::NotFound(_))));
    }

    #[tokio::test]
    async fn rename_empty_name_rejected() {
        let volumes = new_volumes();
        let result = rename("photos", "", &volumes).await;
        assert!(matches!(result, Err(BankError::InvalidName(_))));
    }

    #[tokio::test]
    async fn set_roles_updates_all_volumes() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
        }

        let events = set_roles("photos", vec!["archive".into()], &volumes)
            .await
            .unwrap();
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn set_visibility_updates_all_volumes() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
        }

        let events = set_visibility("photos", StorageVisibility::Closed, &volumes)
            .await
            .unwrap();
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn release_clears_management() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert(
                "/dev/sda1".into(),
                make_managed_volume("/dev/sda1", "/mnt/a", "photos", "rs-1", StorageRole::Primary),
            );
        }

        let (events, paths) = release("photos", &volumes).await.unwrap();
        assert!(!events.is_empty());
        assert_eq!(paths.len(), 1);

        // Volume should no longer be managed
        let bank = by_name("photos", &volumes).await;
        assert!(bank.is_none());
    }

    #[tokio::test]
    async fn offline_volumes_excluded_from_banks() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            let mut vol = make_managed_volume(
                "/dev/sda1",
                "/mnt/a",
                "photos",
                "rs-1",
                StorageRole::Primary,
            );
            vol.disconnect();
            map.insert("/dev/sda1".into(), vol);
        }

        let banks = local_banks(&volumes).await;
        assert!(banks.is_empty());
    }
}
