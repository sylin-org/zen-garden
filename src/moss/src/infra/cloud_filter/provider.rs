//! Cloud Filter provider — implements `Filter` for Windows CfApi callbacks
//! (STORAGE-0012, refactored per STORAGE-0015)
//!
//! Thin CfApi adapter — converts callback arguments to domain types and
//! delegates actual I/O to `StorageHandle`.  No business logic, no
//! duplicated dispatch — every `match Local/Proxy` lives in the handle.
//!
//! ## Callback coverage
//!
//! | Callback              | Status    | Behavior                                   |
//! |-----------------------|-----------|--------------------------------------------|
//! | `fetch_data`          | Active    | Hydrate placeholder via handle             |
//! | `fetch_placeholders`  | Active    | Populate directory via handle               |
//! | `rename`              | Active    | Classify → dispatch via handle              |
//! | `renamed`             | Logging   | Post-rename confirmation                   |
//! | `delete`              | Active    | Propagate via handle, approve              |
//! | `deleted`             | Logging   | Post-delete confirmation                   |
//! | `dehydrate`           | Active    | Approve (free disk space)                  |
//! | `dehydrated`          | Logging   | Post-dehydration confirmation              |
//! | `opened`              | Logging   | Detect corrupt/unsupported metadata        |
//! | `closed`              | Logging   | Detect close-after-delete                  |
//! | `state_changed`       | Logging   | Attribute changes (pin/unpin)              |
//! | `cancel_fetch_*`      | Default   | No-op (CfApi handles cancellation)         |
//! | `validate_data`       | Default   | Not required (no ValidationRequired policy)|

use std::path::PathBuf;

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{Filter, Request, info, ticket};
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::utility::WriteAt;
use tracing::{debug, info, warn};

use crate::domain::cloud_drive::{DriveAction, classify_rename};
use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::domain::storage_service::StorageRoute;
use crate::infra::storage::handle::{self as router, FileEntry, StorageHandle, StorageResolver};
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
    pub(crate) tick: tokio::sync::broadcast::Sender<StorageTick>,
    pub(crate) sync_root_path: PathBuf,
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

    /// List all online storages with availability metadata, sorted by name.
    pub(crate) async fn list_storage_availability(
        &self,
    ) -> Vec<(String, placeholders::StorageAvailability)> {
        let mut avail =
            super::enumerate_storage_availability(&self.volumes, &self.registry, &self.stone_id)
                .await;
        debug!(total = avail.len(), "storage enumeration complete");
        let mut sorted: Vec<String> = avail.keys().cloned().collect();
        sorted.sort();
        sorted
            .into_iter()
            .map(|name| {
                let a = avail.remove(&name).unwrap();
                (name, a)
            })
            .collect()
    }

    /// Check whether a name corresponds to a known storage (local or remote).
    ///
    /// Short-circuits on local match to avoid full enumeration (A11e).
    async fn is_known_storage(&self, name: &str) -> bool {
        if StorageRoute::find_local(name, &self.volumes)
            .await
            .is_some()
        {
            return true;
        }
        let reg = self.registry.read().await;
        reg.storage_entries().iter().any(|e| e.tool.fqid == name)
    }

    /// Build a resolver pre-loaded with this provider's shared state.
    fn resolver(&self) -> StorageResolver<'_> {
        StorageResolver {
            volumes: &self.volumes,
            registry: &self.registry,
            stone_id: &self.stone_id,
            tick: Some(self.tick.clone()),
        }
    }

    /// Resolve a read-capable handle for a storage.
    async fn handle_read(&self, name: &str) -> CResult<StorageHandle> {
        self.resolver().for_read(name).await.map_err(|e| {
            warn!(storage = %name, error = %e, "storage not found for read");
            CloudErrorKind::NotInSync
        })
    }

    /// Resolve a write-capable handle for a storage.
    async fn handle_write(&self, name: &str) -> CResult<StorageHandle> {
        self.resolver().for_write(name).await.map_err(|e| {
            warn!(storage = %name, error = %e, "storage not found for write");
            CloudErrorKind::NotInSync
        })
    }

    /// Rename a top-level storage folder (replica set name) via Explorer.
    ///
    /// Domain mutation via `rename_replica_set`; disk persistence via infra.
    async fn do_rename_storage(&self, old_name: &str, new_name: &str) -> CResult<()> {
        use crate::domain::storage_service::rename_replica_set;

        let mount_paths = rename_replica_set(old_name, new_name, &self.volumes)
            .await
            .map_err(|e| {
                warn!(old = %old_name, new = %new_name, error = %e, "rename rejected");
                CloudErrorKind::NotInSync
            })?;

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

        let router = self.handle_read(&storage_name).await?;

        // Ranged read (A11j): only fetch the bytes CfApi actually needs,
        // avoiding full-file load into memory for large files.
        let range = info.required_file_range();
        let length = range.end.saturating_sub(range.start);
        if length > 0 {
            let data = router
                .read_range(&rel_path, range.start, length)
                .await
                .map_err(cfail)?;
            if !data.is_empty() {
                ticket.write_at(&data, range.start).map_err(|e| {
                    warn!(error = %e, "write_at failed");
                    CloudErrorKind::NotInSync
                })?;
            }
        }

        debug!(storage = %storage_name, path = %rel_path, offset = range.start, length, "hydrated file range");
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
            let entries = self.list_storage_availability().await;
            let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();

            info!(
                storages = ?names,
                sync_root = %self.sync_root_path.display(),
                "fetch_placeholders: enumerating storages for sync root"
            );

            let mut phs: Vec<PlaceholderFile> = entries
                .iter()
                .map(|(n, avail)| placeholders::build_storage_dir_placeholder(n, avail))
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

        let router = self.handle_read(&storage_name).await?;
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

        let router = self.handle_write(&storage_name).await?;
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
                let dst = self.handle_write(&storage).await?;
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
                    let r = self.handle_write(&storage).await?;
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

            DriveAction::RenameInStorage {
                storage,
                old,
                new,
                is_dir,
            } => {
                let r = self.handle_write(&storage).await?;
                r.rename(&old, &new, is_dir).await.map_err(cfail)?;
                debug!(storage = %storage, old = %old, new = %new, is_dir, "renamed within storage");
            }

            DriveAction::CrossStorageMove {
                src_storage,
                src,
                dst_storage,
                dst,
                is_dir,
            } => {
                let src_r = self.handle_read(&src_storage).await?;
                let dst_r = self.handle_write(&dst_storage).await?;
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
                let src_w = self.handle_write(&src_storage).await?;
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
                let dst = self.handle_write(&storage).await?;
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
        debug!(
            count = changes.len(),
            "state_changed: attribute changes detected"
        );
    }
}

// ============================================================================
// Error mapping
// ============================================================================

/// Map any displayable error to a Cloud Filter error.
fn cfail(e: impl std::fmt::Display) -> CloudErrorKind {
    warn!(error = %e, "cloud filter operation failed");
    CloudErrorKind::NotInSync
}
