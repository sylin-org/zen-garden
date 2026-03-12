//! Cloud Filter provider — implements `Filter` for Windows CfApi callbacks
//! (STORAGE-0012, refactored per STORAGE-0015)
//!
//! Thin CfApi adapter — converts callback arguments to domain types and
//! delegates actual I/O to `StorageRouter`.  No business logic, no
//! duplicated dispatch — every `match Local/Proxy` lives in the router.
//!
//! ## Callback coverage
//!
//! | Callback              | Status    | Behavior                                   |
//! |-----------------------|-----------|--------------------------------------------|
//! | `fetch_data`          | Active    | Hydrate placeholder via router             |
//! | `fetch_placeholders`  | Active    | Populate directory via router               |
//! | `rename`              | Active    | Classify → dispatch via router              |
//! | `renamed`             | Logging   | Post-rename confirmation                   |
//! | `delete`              | Active    | Propagate via router, approve              |
//! | `deleted`             | Logging   | Post-delete confirmation                   |
//! | `dehydrate`           | Active    | Approve (free disk space)                  |
//! | `dehydrated`          | Logging   | Post-dehydration confirmation              |
//! | `opened`              | Logging   | Detect corrupt/unsupported metadata        |
//! | `closed`              | Logging   | Detect close-after-delete                  |
//! | `state_changed`       | Logging   | Attribute changes (pin/unpin)              |
//! | `cancel_fetch_*`      | Default   | No-op (CfApi handles cancellation)         |
//! | `validate_data`       | Default   | Not required (no ValidationRequired policy)|

use std::path::PathBuf;
use std::sync::Arc;

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{info, ticket, Filter, Request};
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::utility::WriteAt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::domain::cloud_drive::{classify_rename, DriveAction};
use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::domain::storage_service::StorageRoute;
use crate::infra::storage::router::{self, FileEntry, StorageRouter};
use garden_common::storage::StorageTick;

use super::placeholders;

/// Shared state needed by Cloud Filter callbacks.
///
/// All fields are `Arc`-wrapped so the provider is `Send + Sync + 'static`.
/// Constructed once at startup, lives as long as the `Connection`.
pub struct ZenGardenProvider {
    pub(crate) volumes: Volumes,
    pub(crate) registry: GardenRegistry,
    pub(crate) stone_id: String,
    #[allow(dead_code)]
    pub(crate) tick: tokio::sync::broadcast::Sender<StorageTick>,
    pub(crate) sync_root_path: PathBuf,
    #[allow(dead_code)]
    pub(crate) local_endpoint: Arc<RwLock<String>>,
}

// ============================================================================
// Helper methods
// ============================================================================

impl ZenGardenProvider {
    /// Resolve the storage name and relative path from a Cloud Filter request.
    ///
    /// Layout: `{sync_root}/{storage_name}/{relative_path}`.
    fn resolve_path(&self, request_path: &std::path::Path) -> Option<(String, String)> {
        let (storage_name, remainder) =
            super::decompose_sync_root_path(request_path, &self.sync_root_path)?;
        let rel_path = remainder.to_string_lossy().replace('\\', "/");
        Some((storage_name, rel_path))
    }

    /// List all known storage names (local + remote), sorted.
    pub(crate) async fn list_storage_names(&self) -> Vec<String> {
        let names = super::enumerate_storage_names(&self.volumes, &self.registry).await;
        debug!(total = names.len(), "storage enumeration complete");
        let mut sorted: Vec<String> = names.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Check whether a name corresponds to a known storage (local or remote).
    async fn is_known_storage(&self, name: &str) -> bool {
        super::enumerate_storage_names(&self.volumes, &self.registry)
            .await
            .contains(name)
    }

    /// Build a read-capable router for a storage.
    async fn router_read(&self, name: &str) -> CResult<StorageRouter> {
        StorageRouter::for_read(name, &self.volumes, &self.registry, &self.stone_id)
            .await
            .map_err(|e| {
                warn!(storage = %name, error = %e, "storage not found for read");
                CloudErrorKind::NotInSync
            })
    }

    /// Build a write-capable router for a storage.
    async fn router_write(&self, name: &str) -> CResult<StorageRouter> {
        StorageRouter::for_write(name, &self.volumes, &self.registry, &self.stone_id)
            .await
            .map_err(|e| {
                warn!(storage = %name, error = %e, "storage not found for write");
                CloudErrorKind::NotInSync
            })
    }

    /// Rename a top-level storage folder (replica set name) via Explorer.
    ///
    /// Updates `replica_set_name` on both the in-memory volume and the on-disk
    /// manifest.  The individual device name (`mgmt.name`) is unchanged.
    async fn do_rename_storage(&self, old_name: &str, new_name: &str) -> CResult<()> {
        if StorageRoute::find_local(old_name, &self.volumes).await.is_none() {
            warn!(
                old = %old_name,
                new = %new_name,
                "rename rejected: storage not found locally"
            );
            return Err(CloudErrorKind::NotInSync);
        }

        let mut map = self.volumes.write().await;
        let mut mount_paths = Vec::new();
        for vol in map.values_mut() {
            let matches = vol.management.as_ref().is_some_and(|m| {
                let rs_display = if m.replica_set_name.is_empty() {
                    garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY
                } else {
                    &m.replica_set_name
                };
                rs_display == old_name
            });
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
            warn!(storage = %old_name, "rename: no volumes found");
            return Err(CloudErrorKind::NotInSync);
        }

        for mp in &mount_paths {
            if let Err(e) =
                crate::api::v1::storage::update_manifest_replica_set_name(mp, new_name).await
            {
                warn!(
                    old = %old_name,
                    new = %new_name,
                    mount = %mp,
                    error = %e,
                    "rename: failed to update manifest on disk"
                );
                return Err(CloudErrorKind::NotInSync);
            }
        }

        info!(
            old = %old_name,
            new = %new_name,
            volumes = mount_paths.len(),
            "replica set renamed via Explorer"
        );
        Ok(())
    }
}

// ============================================================================
// Filter trait — all 14 callbacks
// ============================================================================

impl Filter for ZenGardenProvider {
    // ---- Data hydration (download path) ----

    async fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let path = request.path();
        let (storage_name, rel_path) = match self.resolve_path(&path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        if storage_name.is_empty() || rel_path.is_empty() {
            return Ok(());
        }

        let router = self.router_read(&storage_name).await?;
        let data = router.read(&rel_path).await.map_err(cfail)?;

        let range = info.required_file_range();
        let start = range.start as usize;
        let end = std::cmp::min(range.end as usize, data.len());
        if start < end {
            ticket
                .write_at(&data[start..end], range.start)
                .map_err(|e| {
                    warn!(error = %e, "write_at failed");
                    CloudErrorKind::NotInSync
                })?;
        }

        debug!(storage = %storage_name, path = %rel_path, bytes = data.len(), "hydrated file");
        Ok(())
    }

    async fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        _info: info::FetchPlaceholders,
    ) -> CResult<()> {
        let path = request.path();
        let (storage_name, rel_path) = match self.resolve_path(&path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        // Sync root itself — list known storages as directories
        if storage_name.is_empty() {
            let names = self.list_storage_names().await;

            info!(
                storages = ?names,
                sync_root = %self.sync_root_path.display(),
                "fetch_placeholders: enumerating storages for sync root"
            );

            let mut phs: Vec<PlaceholderFile> = names
                .iter()
                .map(|n| placeholders::build_placeholder(n, true, 0))
                .collect();

            if phs.is_empty() {
                debug!("no storages known yet, returning empty placeholder list");
            }

            let count = phs.len();
            ticket.pass_with_placeholder(&mut phs).map_err(|e| {
                warn!(
                    error = %e,
                    count,
                    storages = ?names,
                    sync_root = %self.sync_root_path.display(),
                    "pass_with_placeholder FAILED for sync root"
                );
                CloudErrorKind::NotInSync
            })?;

            for ph in &phs {
                match ph.result() {
                    Ok(usn) => debug!(usn, "placeholder entry OK"),
                    Err(e) => warn!(error = %e, "placeholder entry failed"),
                }
            }

            info!(count, "sync root placeholders created");
            return Ok(());
        }

        // Storage subdirectory
        debug!(storage = %storage_name, path = %rel_path, "fetch_placeholders");

        let router = self.router_read(&storage_name).await?;
        let entries = router.list(&rel_path).await.map_err(cfail)?;

        let filtered: Vec<&FileEntry> = entries
            .iter()
            .filter(|e| !garden_common::constants::storage::share::is_blocked_name(&e.name))
            .collect();

        let mut phs: Vec<PlaceholderFile> = filtered
            .iter()
            .map(|e| placeholders::build_placeholder(&e.name, e.is_dir, e.size))
            .collect();

        ticket.pass_with_placeholder(&mut phs).map_err(|e| {
            warn!(
                storage = %storage_name,
                path = %rel_path,
                count = phs.len(),
                error = %e,
                "pass_with_placeholder failed"
            );
            CloudErrorKind::NotInSync
        })?;

        debug!(
            storage = %storage_name,
            path = %rel_path,
            count = phs.len(),
            "populated placeholders"
        );
        Ok(())
    }

    // ---- File handle lifecycle ----

    async fn opened(&self, request: Request, info: info::Opened) {
        if info.metadata_corrupt() || info.metadata_unsupported() {
            warn!(
                path = %request.path().display(),
                corrupt = info.metadata_corrupt(),
                unsupported = info.metadata_unsupported(),
                "opened: placeholder metadata issue"
            );
        }
    }

    async fn closed(&self, request: Request, info: info::Closed) {
        if info.deleted() {
            debug!(path = %request.path().display(), "closed (deleted)");
        }
    }

    // ---- Dehydration (free disk space) ----

    async fn dehydrate(
        &self,
        request: Request,
        ticket: ticket::Dehydrate,
        info: info::Dehydrate,
    ) -> CResult<()> {
        debug!(
            path = %request.path().display(),
            background = info.background(),
            reason = ?info.reason(),
            "dehydrate approved"
        );
        ticket.pass().map_err(|e| {
            warn!(error = %e, "dehydrate ticket.pass() failed");
            CloudErrorKind::NotInSync
        })?;
        Ok(())
    }

    async fn dehydrated(&self, request: Request, info: info::Dehydrated) {
        debug!(
            path = %request.path().display(),
            background = info.background(),
            reason = ?info.reason(),
            "dehydrated"
        );
    }

    // ---- Delete ----

    async fn delete(
        &self,
        request: Request,
        ticket: ticket::Delete,
        delete_info: info::Delete,
    ) -> CResult<()> {
        let path = request.path();
        let (storage_name, rel_path) = match self.resolve_path(&path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        if storage_name.is_empty() {
            return Err(CloudErrorKind::NotSupported);
        }

        if rel_path.is_empty() {
            warn!(
                storage = %storage_name,
                "delete rejected: use 'rake storage release' to remove a storage"
            );
            return Err(CloudErrorKind::NotSupported);
        }

        let router = self.router_write(&storage_name).await?;
        if delete_info.is_directory() {
            router.delete_dir(&rel_path).await.map_err(cfail)?;
        } else {
            router.delete_file(&rel_path).await.map_err(cfail)?;
        }

        ticket.pass().map_err(|e| {
            warn!(error = %e, "delete ticket.pass() failed");
            CloudErrorKind::NotInSync
        })?;

        info!(storage = %storage_name, path = %rel_path, "delete approved and propagated");
        Ok(())
    }

    async fn deleted(&self, request: Request, _info: info::Deleted) {
        debug!(path = %request.path().display(), "deleted (post-completion)");
    }

    // ---- Rename / Move ----

    async fn rename(
        &self,
        request: Request,
        ticket: ticket::Rename,
        rename_info: info::Rename,
    ) -> CResult<()> {
        let source_path = request.path();
        let target_path = rename_info.target_path();

        debug!(
            source = %source_path.display(),
            target = %target_path.display(),
            is_dir = rename_info.is_directory(),
            source_in_scope = rename_info.source_in_scope(),
            target_in_scope = rename_info.target_in_scope(),
            "rename callback"
        );

        // Resolve both paths (may be None for out-of-scope ends)
        let (old_storage, old_rel) = self
            .resolve_path(&source_path)
            .unwrap_or_else(|| (String::new(), String::new()));
        let (new_storage, new_rel) = self
            .resolve_path(&target_path)
            .unwrap_or_else(|| (String::new(), String::new()));

        let is_known = if old_storage.is_empty() {
            false
        } else {
            self.is_known_storage(&old_storage).await
        };

        let action = classify_rename(
            rename_info.source_in_scope(),
            rename_info.target_in_scope(),
            &old_storage,
            &old_rel,
            &new_storage,
            &new_rel,
            rename_info.is_directory(),
            is_known,
            &source_path,
            &self.sync_root_path,
        );

        debug!(action = ?action, "classify_rename result");

        match action {
            DriveAction::IngestFromOutside {
                source,
                storage,
                path,
                is_dir,
            } => {
                let dst = self.router_write(&storage).await?;
                router::ingest(&source, &dst, &path, is_dir)
                    .await
                    .map_err(cfail)?;
                info!(storage = %storage, path = %path, "ingested from outside sync root");
            }

            DriveAction::DeleteFromStorage {
                storage,
                path,
                is_dir,
            } => {
                // Best-effort: user moved item out of sync root
                let result = async {
                    let r = self.router_write(&storage).await?;
                    if is_dir {
                        r.delete_dir(&path).await.map_err(cfail)
                    } else {
                        r.delete_file(&path).await.map_err(cfail)
                    }
                }
                .await;
                if let Err(e) = result {
                    warn!(
                        storage = %storage,
                        path = %path,
                        error = ?e,
                        "rename-out: propagation failed, approving anyway"
                    );
                }
                info!(storage = %storage, path = %path, "file moved out of sync root");
            }

            DriveAction::RenameInStorage { storage, old, new } => {
                let r = self.router_write(&storage).await?;
                r.rename(&old, &new).await.map_err(cfail)?;
                debug!(storage = %storage, old = %old, new = %new, "renamed within storage");
            }

            DriveAction::CrossStorageMove {
                src_storage,
                src,
                dst_storage,
                dst,
                is_dir,
            } => {
                let src_r = self.router_read(&src_storage).await?;
                let dst_r = self.router_write(&dst_storage).await?;
                if is_dir {
                    router::transfer_tree(&src_r, &src, &dst_r, &dst)
                        .await
                        .map_err(cfail)?;
                } else {
                    router::transfer(&src_r, &src, &dst_r, &dst)
                        .await
                        .map_err(cfail)?;
                }
                // Delete source after successful copy
                let src_w = self.router_write(&src_storage).await?;
                if is_dir {
                    src_w.delete_dir(&src).await.map_err(cfail)?;
                } else {
                    src_w.delete_file(&src).await.map_err(cfail)?;
                }
                info!(
                    src_storage = %src_storage, src_path = %src,
                    dst_storage = %dst_storage, dst_path = %dst,
                    "cross-storage move completed"
                );
            }

            DriveAction::RenameStorage { old_name, new_name } => {
                self.do_rename_storage(&old_name, &new_name).await?;
            }

            DriveAction::IngestStray {
                stray_path,
                storage,
                path,
                is_dir,
            } => {
                let dst = self.router_write(&storage).await?;
                router::ingest(&stray_path, &dst, &path, is_dir)
                    .await
                    .map_err(cfail)?;
                info!(
                    stray = %stray_path.display(),
                    storage = %storage,
                    path = %path,
                    "ingested stray root item"
                );
            }

            DriveAction::Reject { reason } => {
                warn!(reason, "rename rejected by policy");
                return Err(CloudErrorKind::NotSupported);
            }
        }

        ticket.pass().map_err(|e| {
            warn!(error = %e, "rename ticket.pass() failed");
            CloudErrorKind::NotInSync
        })?;
        Ok(())
    }

    async fn renamed(&self, request: Request, rename_info: info::Renamed) {
        debug!(
            source = %rename_info.source_path().display(),
            target = %request.path().display(),
            "renamed (post-completion)"
        );
    }

    // ---- State changes (attribute monitoring) ----

    async fn state_changed(&self, changes: Vec<PathBuf>) {
        debug!(count = changes.len(), "state_changed: attribute changes detected");
    }
}

// ============================================================================
// Error mapping
// ============================================================================

/// Map an anyhow error to a Cloud Filter error.
fn cfail(e: anyhow::Error) -> CloudErrorKind {
    warn!(error = %e, "cloud filter operation failed");
    CloudErrorKind::NotInSync
}
