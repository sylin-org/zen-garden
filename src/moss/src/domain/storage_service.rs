//! Storage domain service (STORAGE-0009)
//!
//! Single entry point for all storage operations. Consolidates the
//! "find storage, check role, route or execute" pattern that was
//! previously reimplemented in every handler (memories, S3, garden
//! storage, stone storage, storage gateway).
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
//! WebDAV responses). This service returns domain results that handlers
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
    /// Storage ID (GUIDv7).
    pub id: String,
    /// Logical storage name.
    pub name: String,
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
// StorageService
// ============================================================================

/// Domain service for storage resolution and routing.
///
/// Constructed per-request from shared state references. Cheap to create
/// (no allocations, no I/O — just borrows).
pub struct StorageService<'a> {
    volumes: &'a Volumes,
    registry: &'a GardenRegistry,
    stone_id: &'a str,
    tick_tx: Option<&'a broadcast::Sender<StorageTick>>,
}

impl<'a> StorageService<'a> {
    /// Create a new StorageService from shared state references.
    pub fn new(
        volumes: &'a Volumes,
        registry: &'a GardenRegistry,
        stone_id: &'a str,
        tick_tx: Option<&'a broadcast::Sender<StorageTick>>,
    ) -> Self {
        Self {
            volumes,
            registry,
            stone_id,
            tick_tx,
        }
    }

    // ========================================================================
    // Resolution
    // ========================================================================

    /// Resolve a storage name to a route decision for **read** operations.
    ///
    /// Read routing: local storage is used regardless of role (Primary or
    /// Dormant can both serve reads). If not found locally, routes to the
    /// remote Primary.
    pub async fn resolve_read(&self, name: &str) -> Result<StorageRoute> {
        // Try local first — any role can serve reads
        if let Some(local) = self.find_local(name).await {
            return Ok(StorageRoute::Local(local));
        }

        // Not local — find any remote replica
        self.find_remote(name)
            .await
            .context(format!("Storage '{}' not available", name))
    }

    /// Resolve a storage name to a route decision for **write** operations.
    ///
    /// Write routing: only the Primary replica accepts writes. If the local
    /// storage is Dormant, the request is proxied to the remote Primary.
    pub async fn resolve_write(&self, name: &str) -> Result<StorageRoute> {
        // Try local — only Primary can accept writes
        if let Some(local) = self.find_local(name).await {
            if local.role == StorageRole::Primary {
                return Ok(StorageRoute::Local(local));
            }

            debug!(
                storage = %name,
                "Local storage is Dormant, routing write to remote Primary"
            );
        }

        // Local is absent or Dormant — find remote Primary
        self.find_remote_primary(name)
            .await
            .context(format!("No Primary replica for storage '{}'", name))
    }

    /// Resolve a storage name for **stone-local** operations only.
    ///
    /// Returns the local storage if present, regardless of role.
    /// Never proxies. Used by stone-tier admin endpoints.
    pub async fn resolve_local(&self, name: &str) -> Option<LocalStorage> {
        self.find_local(name).await
    }

    /// Resolve a storage by ID for **stone-local** operations.
    pub async fn resolve_local_by_id(&self, id: &str) -> Option<LocalStorage> {
        self.find_local_by_id(id).await
    }

    /// List all locally-managed storages.
    pub async fn list_local(&self) -> Vec<LocalStorage> {
        let map = self.volumes.read().await;
        map.values()
            .filter_map(|vol| {
                let mgmt = vol.management.as_ref()?;
                Some(LocalStorage {
                    id: mgmt.id.clone(),
                    name: mgmt.name.clone(),
                    mount_path: vol.mount_path.clone(),
                    role: mgmt.role,
                    encrypted: mgmt.encrypted,
                    roles: mgmt.roles.clone(),
                })
            })
            .collect()
    }

    // ========================================================================
    // Store construction
    // ========================================================================

    /// Build a read-only `ContentStore` for the given local storage.
    pub fn content_store(&self, local: &LocalStorage) -> ContentStore {
        ContentStore::new(local.mount_path.clone(), None)
    }

    /// Build a `ContentStore` with changelog notifications (for writes).
    ///
    /// The notification channel feeds the SSE doorbell and replication task.
    pub fn notifying_content_store(&self, local: &LocalStorage) -> ContentStore {
        let store = ContentStore::new(local.mount_path.clone(), None);
        if let Some(tx) = self.tick_tx {
            store.with_notifications(local.name.clone(), tx.clone())
        } else {
            store
        }
    }

    /// Build a read-only `ObjectStore` for the given local storage.
    pub fn object_store(&self, local: &LocalStorage) -> ObjectStore {
        ObjectStore::new(&local.mount_path)
    }

    /// Build an `ObjectStore` with changelog notifications (for writes).
    pub fn notifying_object_store(&self, local: &LocalStorage) -> ObjectStore {
        ObjectStore::with_store(self.notifying_content_store(local))
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Find a managed storage by name in the local `Volumes` map.
    async fn find_local(&self, name: &str) -> Option<LocalStorage> {
        let map = self.volumes.read().await;
        map.values().find_map(|vol| {
            let mgmt = vol.management.as_ref()?;
            if mgmt.name != name {
                return None;
            }
            Some(LocalStorage {
                id: mgmt.id.clone(),
                name: mgmt.name.clone(),
                mount_path: vol.mount_path.clone(),
                role: mgmt.role,
                encrypted: mgmt.encrypted,
                roles: mgmt.roles.clone(),
            })
        })
    }

    /// Find a managed storage by ID in the local `Volumes` map.
    async fn find_local_by_id(&self, id: &str) -> Option<LocalStorage> {
        let map = self.volumes.read().await;
        map.values().find_map(|vol| {
            let mgmt = vol.management.as_ref()?;
            if mgmt.id != id {
                return None;
            }
            Some(LocalStorage {
                id: mgmt.id.clone(),
                name: mgmt.name.clone(),
                mount_path: vol.mount_path.clone(),
                role: mgmt.role,
                encrypted: mgmt.encrypted,
                roles: mgmt.roles.clone(),
            })
        })
    }

    /// Find any remote replica (Primary preferred) via the GardenRegistry.
    async fn find_remote(&self, name: &str) -> Option<StorageRoute> {
        let reg = self.registry.read().await;
        reg.route_to_primary(name, self.stone_id)
            .map(|(stone_id, endpoint, _bank_id)| {
                StorageRoute::Proxy(ProxyTarget {
                    endpoint,
                    stone_id,
                })
            })
    }

    /// Find specifically the remote Primary replica.
    async fn find_remote_primary(&self, name: &str) -> Option<StorageRoute> {
        let reg = self.registry.read().await;

        // route_to_primary already prefers Primary role, falls back to any
        reg.route_to_primary(name, self.stone_id)
            .map(|(stone_id, endpoint, _bank_id)| {
                StorageRoute::Proxy(ProxyTarget {
                    endpoint,
                    stone_id,
                })
            })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::garden_registry::new_registry;
    use crate::domain::storage::{new_volumes, Management, Volume, VolumeHealth};
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
            online: true,
            health: VolumeHealth::Healthy,
            management: Some(Management {
                id: id.to_string(),
                short_id: id[..8.min(id.len())].to_string(),
                name: name.to_string(),
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

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let route = svc.resolve_read("photos").await.unwrap();
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

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let route = svc.resolve_read("photos").await.unwrap();
        assert!(matches!(route, StorageRoute::Local(l) if l.role == StorageRole::Dormant));
    }

    #[tokio::test]
    async fn test_resolve_read_not_found_returns_error() {
        let volumes = new_volumes();
        let registry = new_registry();

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let result = svc.resolve_read("nonexistent").await;
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

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let route = svc.resolve_write("photos").await.unwrap();
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

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let result = svc.resolve_write("photos").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_local_returns_none_for_missing() {
        let volumes = new_volumes();
        let registry = new_registry();

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        assert!(svc.resolve_local("missing").await.is_none());
    }

    #[tokio::test]
    async fn test_resolve_local_by_id() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-123", "data", StorageRole::Primary, "/mnt/data"));
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let found = svc.resolve_local_by_id("id-123").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "data");

        assert!(svc.resolve_local_by_id("id-999").await.is_none());
    }

    #[tokio::test]
    async fn test_list_local_returns_all() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "photos", StorageRole::Primary, "/mnt/photos"));
            map.insert("/dev/sdb1".into(), make_volume("id-2", "backups", StorageRole::Dormant, "/mnt/backups"));
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let all = svc.list_local().await;
        assert_eq!(all.len(), 2);

        let names: Vec<&str> = all.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"photos"));
        assert!(names.contains(&"backups"));
    }

    #[tokio::test]
    async fn test_content_store_construction() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let local = svc.resolve_local("data").await.unwrap();

        let store = svc.content_store(&local);
        assert_eq!(store.mount_root(), PathBuf::from("/mnt/data"));
        assert!(!store.is_encrypted());
    }

    #[tokio::test]
    async fn test_notifying_store_without_tx() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let local = svc.resolve_local("data").await.unwrap();
        let _store = svc.notifying_content_store(&local);
    }

    #[tokio::test]
    async fn test_notifying_store_with_tx() {
        let volumes = new_volumes();
        let registry = new_registry();
        let (tx, _rx) = broadcast::channel::<StorageTick>(16);
        {
            let mut map = volumes.write().await;
            map.insert("/dev/sda1".into(), make_volume("id-1", "data", StorageRole::Primary, "/mnt/data"));
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", Some(&tx));
        let local = svc.resolve_local("data").await.unwrap();
        let _store = svc.notifying_content_store(&local);
    }

    #[tokio::test]
    async fn test_local_storage_carries_metadata() {
        let volumes = new_volumes();
        let registry = new_registry();
        {
            let mut map = volumes.write().await;
            let mut vol = make_volume("id-1", "encrypted-bank", StorageRole::Primary, "/mnt/enc");
            if let Some(ref mut mgmt) = vol.management {
                mgmt.encrypted = true;
                mgmt.roles = vec!["seed-bank".into(), "archive".into()];
            }
            map.insert("/dev/sda1".into(), vol);
        }

        let svc = StorageService::new(&volumes, &registry, "stone-01", None);
        let local = svc.resolve_local("encrypted-bank").await.unwrap();
        assert!(local.encrypted);
        assert_eq!(local.roles, vec!["seed-bank", "archive"]);
        assert_eq!(local.id, "id-1");
    }
}
