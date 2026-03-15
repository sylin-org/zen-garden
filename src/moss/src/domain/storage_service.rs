//! Storage domain routing (STORAGE-0009)
//!
//! Associated functions on `StorageRoute` and `LocalStorage` replace the
//! former `StorageService<'a>` struct.  Callers pass the explicit state
//! references they already hold rather than constructing an intermediate object.
//!
//! ## Responsibilities
//!
//! - **Resolution**: find a storage by name from local `ManagedStorages`
//!   or remote `GardenRegistry`
//! - **Routing**: decide Local (Primary) vs Proxy (Remote) based on role
//! - **Store construction**: build `ContentStore` / `ObjectStore` from
//!   the resolved local storage
//!
//! ## Non-responsibilities
//!
//! Handlers still own serialization format (XML for S3, JSON for REST,
//! WebDAV responses). This module returns domain results that handlers
//! map to their wire format.

use std::path::PathBuf;

use anyhow::{Context, Result};
use garden_common::storage::StorageRole;
use tracing::debug;

use tokio::sync::broadcast;

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::infra::storage::{ContentStore, ObjectStore};
use garden_common::storage::StorageTick;

// ============================================================================
// Route decision
// ============================================================================

/// Routing decision after resolving a storage name.
#[derive(Debug)]
pub enum StorageRoute {
    /// Execute locally — this stone hosts the Primary (or is the only replica).
    Local(LocalStorage),
    /// Proxy to the remote stone hosting the Primary.
    Proxy(ProxyTarget),
}

/// A locally-resolved storage with everything needed for I/O.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Device ID (GUIDv7).
    pub id: String,
    /// Device display name (sugar).
    pub name: String,
    /// Replica set ID (GUIDv7) — groups devices replicating same content.
    pub replica_set_id: String,
    /// Replica set display name. Empty = default set ("storage").
    pub replica_set_name: String,
    /// Mount path on this stone.
    pub mount_path: PathBuf,
    /// Current role (Primary / Dormant).
    pub role: StorageRole,
    /// Whether content is encrypted.
    pub encrypted: bool,
    /// Composable roles (e.g., ["seed-bank"]).
    pub roles: Vec<String>,
}

/// Remote stone to proxy the request to.
#[derive(Debug, Clone)]
pub struct ProxyTarget {
    /// Base HTTP endpoint of the remote stone (e.g. `http://stone-name:7185`).
    pub endpoint: String,
    /// Stone ID of the remote stone.
    pub stone_id: String,
}

// ============================================================================
// StorageRoute — resolution and listing
// ============================================================================

impl StorageRoute {
    /// Resolve a storage name to a route decision for **read** operations.
    ///
    /// Read routing: local storage is used regardless of role (Primary or
    /// Dormant can both serve reads). If not found locally, routes to the
    /// remote Primary.
    pub async fn for_read(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<StorageRoute> {
        // Try local first — any role can serve reads
        if let Some(local) = find_local(name, volumes).await {
            return Ok(StorageRoute::Local(local));
        }

        // Not local — find any remote replica
        find_remote(name, registry, stone_id)
            .await
            .context(format!("Storage '{}' not available", name))
    }

    /// Resolve a storage name to a route decision for **write** operations.
    ///
    /// Write routing: only the Primary replica accepts writes. If the local
    /// storage is Dormant, the request is proxied to the remote Primary.
    pub async fn for_write(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<StorageRoute> {
        // Try local — only Primary can accept writes
        if let Some(local) = find_local(name, volumes).await {
            if local.role == StorageRole::Primary {
                return Ok(StorageRoute::Local(local));
            }

            debug!(
                storage = %name,
                "Local storage is Dormant, routing write to remote Primary"
            );
        }

        // Local is absent or Dormant — find remote Primary
        find_remote(name, registry, stone_id)
            .await
            .context(format!("No Primary replica for storage '{}'", name))
    }

    /// Find a storage by name for **stone-local** operations only.
    ///
    /// Returns the local storage if present, regardless of role.
    /// Never proxies. Used by stone-tier admin endpoints.
    pub async fn find_local(name: &str, volumes: &Volumes) -> Option<LocalStorage> {
        find_local(name, volumes).await
    }

    /// Find a storage by ID for **stone-local** operations.
    pub async fn find_local_by_id(id: &str, volumes: &Volumes) -> Option<LocalStorage> {
        find_local_by_id(id, volumes).await
    }

    /// List all locally-managed storages.
    pub async fn list_local(volumes: &Volumes) -> Vec<LocalStorage> {
        let map = volumes.read().await;
        map.values()
            .filter_map(|vol| {
                let mgmt = vol.management.as_ref()?;
                Some(LocalStorage {
                    id: mgmt.id.clone(),
                    name: mgmt.name.clone(),
                    replica_set_id: mgmt.replica_set_id.clone(),
                    replica_set_name: mgmt.replica_set_name.clone(),
                    mount_path: vol.mount_path.clone(),
                    role: mgmt.role,
                    encrypted: mgmt.encrypted,
                    roles: mgmt.roles.clone(),
                })
            })
            .collect()
    }
}

// ============================================================================
// LocalStorage — store construction
// ============================================================================

impl LocalStorage {
    /// Build a read-only `ContentStore` for this local storage.
    pub fn content_store(&self) -> ContentStore {
        ContentStore::new(self.mount_path.clone(), None)
    }

    /// Build a `ContentStore` with changelog notifications (for writes).
    ///
    /// The notification channel feeds the SSE doorbell and replication task.
    /// Pass `None` to get a plain store without notifications.
    pub fn notifying_content_store(
        &self,
        tick: Option<&broadcast::Sender<StorageTick>>,
    ) -> ContentStore {
        let store = ContentStore::new(self.mount_path.clone(), None);
        if let Some(tx) = tick {
            store.with_notifications(self.name.clone(), self.replica_set_id.clone(), tx.clone())
        } else {
            store
        }
    }

    /// Build a read-only `ObjectStore` for this local storage.
    pub fn object_store(&self) -> ObjectStore {
        ObjectStore::new(&self.mount_path)
    }

    /// Build an `ObjectStore` with changelog notifications (for writes).
    pub fn notifying_object_store(
        &self,
        tick: Option<&broadcast::Sender<StorageTick>>,
    ) -> ObjectStore {
        ObjectStore::with_store(self.notifying_content_store(tick))
    }
}

// ============================================================================
// Private free functions
// ============================================================================

/// Find a managed storage by replica set name in the local `Volumes` map.
///
/// Matches on `replica_set_name` (the user-facing identity).
/// Falls back to DEFAULT_REPLICA_SET_DISPLAY for empty replica set names.
/// Only returns volumes that are currently online — offline volumes are
/// not routable; callers fall back to remote replicas.
async fn find_local(name: &str, volumes: &Volumes) -> Option<LocalStorage> {
    let map = volumes.read().await;
    map.values().find_map(|vol| {
        if !vol.state.is_online() {
            return None;
        }
        let mgmt = vol.management.as_ref()?;
        if mgmt.display_name() != name {
            return None;
        }
        Some(LocalStorage {
            id: mgmt.id.clone(),
            name: mgmt.name.clone(),
            replica_set_id: mgmt.replica_set_id.clone(),
            replica_set_name: mgmt.replica_set_name.clone(),
            mount_path: vol.mount_path.clone(),
            role: mgmt.role,
            encrypted: mgmt.encrypted,
            roles: mgmt.roles.clone(),
        })
    })
}

/// Find a managed storage by ID in the local `Volumes` map.
async fn find_local_by_id(id: &str, volumes: &Volumes) -> Option<LocalStorage> {
    let map = volumes.read().await;
    map.values().find_map(|vol| {
        let mgmt = vol.management.as_ref()?;
        if mgmt.id != id {
            return None;
        }
        Some(LocalStorage {
            id: mgmt.id.clone(),
            name: mgmt.name.clone(),
            replica_set_id: mgmt.replica_set_id.clone(),
            replica_set_name: mgmt.replica_set_name.clone(),
            mount_path: vol.mount_path.clone(),
            role: mgmt.role,
            encrypted: mgmt.encrypted,
            roles: mgmt.roles.clone(),
        })
    })
}

/// Find a remote replica (Primary preferred) via the GardenRegistry.
///
/// `route_to_primary` already prefers the Primary role and falls back to any
/// available replica, so a single function serves both read and write routing.
async fn find_remote(
    name: &str,
    registry: &GardenRegistry,
    stone_id: &str,
) -> Option<StorageRoute> {
    let reg = registry.read().await;
    reg.route_to_primary(name, stone_id)
        .map(|(stone_id, endpoint, _bank_id)| {
            StorageRoute::Proxy(ProxyTarget {
                endpoint,
                stone_id,
            })
        })
}

// ============================================================================
// Replica set rename (domain: in-memory mutation)
// ============================================================================

/// Rename a replica set in the local volumes map.
///
/// Returns the mount paths of affected volumes so the caller can persist
/// the change to disk (infra concern).  Returns `Err` if no local volumes
/// matched `old_name`.
pub async fn rename_replica_set(
    old_name: &str,
    new_name: &str,
    volumes: &Volumes,
) -> Result<Vec<String>> {
    if find_local(old_name, volumes).await.is_none() {
        anyhow::bail!("storage '{}' not found locally", old_name);
    }

    let mut map = volumes.write().await;
    let mut mount_paths = Vec::new();
    for vol in map.values_mut() {
        let matches = vol
            .management
            .as_ref()
            .is_some_and(|m| m.display_name() == old_name);
        if matches {
            mount_paths.push(vol.mount_path.to_string_lossy().to_string());
            if let Some(ref mut mgmt) = vol.management {
                mgmt.replica_set_name = new_name.to_string();
                mgmt.replica_set_name_updated_at = Some(chrono::Utc::now());
            }
        }
    }
    drop(map);

    if mount_paths.is_empty() {
        anyhow::bail!("no volumes found for storage '{}'", old_name);
    }

    Ok(mount_paths)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::garden_registry::new_registry;
    use crate::domain::storage::{new_volumes, Management, Volume, VolumeState};
    use crate::infra::storage::ContentStore;
    use garden_common::storage::{StorageRole, StorageVisibility};
    use std::path::PathBuf;

    /// Helper: build a test Volume with management configured.
    fn make_volume(id: &str, name: &str, role: StorageRole, mount: &str) -> Volume {
        Volume {
            path: "/dev/sda1".to_string(),
            mount_path: PathBuf::from(mount),
            label: None,
            capacity_bytes: 100_000_000_000,
            used_bytes: 10_000_000_000,
            removable: true,
            state: VolumeState::Online,
            management: Some(Management {
                id: id.to_string(),
                short_id: id[..8.min(id.len())].to_string(),
                name: name.to_string(),
                replica_set_id: format!("rs-{}", id),
                replica_set_name: name.to_string(),
                replica_set_name_updated_at: None,
                encrypted: false,
                roaming: false,
                origin_stone: "stone-test".to_string(),
                created_at: chrono::Utc::now(),
                visibility: StorageVisibility::Open,
                roles: vec!["seed-bank".to_string()],
                role,
                pin: None,
                store: ContentStore::new_public(mount),
            }),
        }
    }

    #[tokio::test]
    async fn test_resolve_read_finds_local_primary() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Primary, "/mnt/photos"));
        }

        let route = StorageRoute::for_read("photos", &volumes, &registry, "stone-01").await.unwrap();
        assert!(matches!(route, StorageRoute::Local(l) if l.name == "photos"));
    }

    #[tokio::test]
    async fn test_resolve_read_accepts_dormant() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Dormant, "/mnt/photos"));
        }

        let route = StorageRoute::for_read("photos", &volumes, &registry, "stone-01").await.unwrap();
        assert!(matches!(route, StorageRoute::Local(l) if l.role == StorageRole::Dormant));
    }

    #[tokio::test]
    async fn test_resolve_read_not_found_returns_error() {
        let volumes = new_volumes();
        let registry = new_registry();

        let result = StorageRoute::for_read("nonexistent", &volumes, &registry, "stone-01").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_write_accepts_primary() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Primary, "/mnt/photos"));
        }

        let route = StorageRoute::for_write("photos", &volumes, &registry, "stone-01").await.unwrap();
        assert!(matches!(route, StorageRoute::Local(l) if l.role == StorageRole::Primary));
    }

    #[tokio::test]
    async fn test_resolve_write_rejects_dormant_no_remote() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Dormant, "/mnt/photos"));
        }

        let result = StorageRoute::for_write("photos", &volumes, &registry, "stone-01").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_local_returns_none_for_missing() {
        let volumes = new_volumes();

        assert!(StorageRoute::find_local("missing", &volumes).await.is_none());
    }

    #[tokio::test]
    async fn test_resolve_local_by_id() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-123", "data", StorageRole::Primary, "/mnt/data"));
        }

        let found = StorageRoute::find_local_by_id("id-123", &volumes).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "data");

        assert!(StorageRoute::find_local_by_id("id-999", &volumes).await.is_none());
    }

    #[tokio::test]
    async fn test_list_local_returns_all() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Primary, "/mnt/photos"));
            map.insert("/dev/sdb1".into(), make_volume("id-2", "backups", StorageRole::Dormant, "/mnt/backups"));
        }

        let all = StorageRoute::list_local(&volumes).await;
        assert_eq!(all.len(), 2);

        let names: Vec<&str> = all.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"photos"));
        assert!(names.contains(&"backups"));
    }

    #[tokio::test]
    async fn test_content_store_construction() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let local = StorageRoute::find_local("data", &volumes).await.unwrap();

        let store = local.content_store();
        assert_eq!(store.mount_root(), PathBuf::from("/mnt/data"));
        assert!(!store.is_encrypted());
    }

    #[tokio::test]
    async fn test_notifying_store_without_tx() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let local = StorageRoute::find_local("data", &volumes).await.unwrap();
        let _store = local.notifying_content_store(None);
    }

    #[tokio::test]
    async fn test_notifying_store_with_tx() {
        let volumes = new_volumes();
        let (tx, _rx) = broadcast::channel::<StorageTick>(16);
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let local = StorageRoute::find_local("data", &volumes).await.unwrap();
        let _store = local.notifying_content_store(Some(&tx));
    }

    #[tokio::test]
    async fn test_local_storage_carries_metadata() {
        let volumes = new_volumes();
        {
            let mut map = volumes.write().await;
            let mut vol = make_volume("id-1", "encrypted-bank", StorageRole::Primary, "/mnt/enc");
            if let Some(ref mut mgmt) = vol.management {
                mgmt.encrypted = true;
                mgmt.roles = vec!["seed-bank".into(), "archive".into()];
            }
            map.insert("/dev/sda1".into(), vol);
        }

        let local = StorageRoute::find_local("encrypted-bank", &volumes).await.unwrap();
        assert!(local.encrypted);
        assert_eq!(local.roles, vec!["seed-bank", "archive"]);
        assert_eq!(local.id, "id-1");
    }
}
