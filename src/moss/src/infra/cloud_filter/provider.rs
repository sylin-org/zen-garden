//! Cloud Filter provider — implements `Filter` for Windows CfApi callbacks
//! (STORAGE-0012)
//!
//! Each callback delegates to the Moss storage API (local or proxied)
//! via `StorageService` (domain).  Pure adapter — no business logic.
//!
//! ## Callback coverage
//!
//! | Callback              | Status    | Behavior                                   |
//! |-----------------------|-----------|--------------------------------------------|
//! | `fetch_data`          | Active    | Hydrate placeholder from mount or proxy    |
//! | `fetch_placeholders`  | Active    | Populate directory from mount or proxy      |
//! | `rename`              | Active    | Propagate to mount; out-of-scope = delete  |
//! | `renamed`             | Logging   | Post-rename confirmation                   |
//! | `delete`              | Active    | Propagate to mount, approve                |
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

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::domain::storage_service::StorageRoute;
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

/// Simplified directory entry for placeholder creation.
pub(crate) struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
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

    /// Build a reusable HTTP client for proxy requests.
    fn http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default()
    }

    /// List all known storages (local + remote) as directory entries.
    ///
    /// Uses replica set names as the user-facing identity.
    pub(crate) async fn list_storages(&self) -> Vec<DirEntry> {
        let names = super::enumerate_storage_names(&self.volumes, &self.registry).await;
        debug!(total = names.len(), "storage enumeration complete");

        let mut entries: Vec<DirEntry> = names
            .into_iter()
            .map(|name| DirEntry {
                name,
                is_dir: true,
                size: 0,
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    // ========================================================================
    // Domain operations (fetch, delete, rename)
    // ========================================================================

    /// Fetch file content and write it to the placeholder.
    async fn do_fetch_data(
        &self,
        storage_name: &str,
        rel_path: &str,
        ticket: &ticket::FetchData,
        info: &info::FetchData,
    ) -> CResult<()> {
        let route = StorageRoute::for_read(storage_name, &self.volumes, &self.registry, &self.stone_id).await.map_err(|e| {
            warn!(storage = %storage_name, error = %e, "storage not found for fetch");
            CloudErrorKind::NotInSync
        })?;

        let data = match route {
            StorageRoute::Local(local) => {
                let full_path = local.mount_path.join(rel_path);
                debug!(path = %full_path.display(), "reading local file");
                tokio::fs::read(&full_path).await.map_err(|e| {
                    warn!(path = %full_path.display(), error = %e, "local read failed");
                    CloudErrorKind::NotInSync
                })?
            }
            StorageRoute::Proxy(target) => {
                let url = format!(
                    "{}/api/v1/garden/storage/{}/files/{}",
                    target.endpoint.trim_end_matches('/'),
                    storage_name,
                    rel_path
                );
                debug!(url = %url, "proxying fetch to remote");
                let resp = Self::http_client().get(&url).send().await.map_err(|e| {
                    warn!(error = %e, "proxy fetch failed");
                    CloudErrorKind::NetworkUnavailable
                })?;

                resp.bytes().await.map_err(|e| {
                    warn!(error = %e, "proxy read body failed");
                    CloudErrorKind::NetworkUnavailable
                })?.to_vec()
            }
        };

        let range = info.required_file_range();
        let start = range.start as usize;
        let end = std::cmp::min(range.end as usize, data.len());
        if start < end {
            ticket.write_at(&data[start..end], range.start).map_err(|e| {
                warn!(error = %e, "write_at failed");
                CloudErrorKind::NotInSync
            })?;
        }

        debug!(storage = %storage_name, path = %rel_path, bytes = data.len(), "hydrated file");
        Ok(())
    }

    /// Populate a directory placeholder with child entries.
    async fn do_fetch_placeholders(
        &self,
        storage_name: &str,
        rel_path: &str,
        ticket: &ticket::FetchPlaceholders,
    ) -> CResult<()> {
        let route = StorageRoute::for_read(storage_name, &self.volumes, &self.registry, &self.stone_id).await.map_err(|e| {
            warn!(storage = %storage_name, error = %e, "storage not found for placeholders");
            CloudErrorKind::NotInSync
        })?;

        let entries = match route {
            StorageRoute::Local(local) => {
                let dir_path = if rel_path.is_empty() {
                    local.mount_path.clone()
                } else {
                    local.mount_path.join(rel_path)
                };
                Self::list_local_dir(&dir_path).await?
            }
            StorageRoute::Proxy(target) => {
                Self::list_remote_dir(&target.endpoint, storage_name, rel_path).await?
            }
        };

        let filtered: Vec<&DirEntry> = entries
            .iter()
            .filter(|e| !garden_common::constants::is_blocked_name(&e.name))
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

    /// Delete a file/directory from the storage mount (or proxy to remote).
    async fn do_delete(&self, storage_name: &str, rel_path: &str, is_dir: bool) -> CResult<()> {
        let route = StorageRoute::for_write(storage_name, &self.volumes, &self.registry, &self.stone_id).await.map_err(|e| {
            warn!(storage = %storage_name, error = %e, "storage not found for delete");
            CloudErrorKind::NotInSync
        })?;

        match route {
            StorageRoute::Local(local) => {
                let target = local.mount_path.join(rel_path);
                if !target.exists() {
                    return Ok(()); // already gone
                }
                if is_dir {
                    tokio::fs::remove_dir_all(&target).await.map_err(|e| {
                        warn!(path = %target.display(), error = %e, "delete: rmdir failed");
                        CloudErrorKind::NotInSync
                    })?;
                } else {
                    tokio::fs::remove_file(&target).await.map_err(|e| {
                        warn!(path = %target.display(), error = %e, "delete: rm failed");
                        CloudErrorKind::NotInSync
                    })?;
                }
                debug!(storage = %storage_name, path = %rel_path, "deleted from mount");
            }
            StorageRoute::Proxy(target) => {
                let url = format!(
                    "{}/api/v1/garden/storage/{}/files/{}",
                    target.endpoint.trim_end_matches('/'),
                    storage_name,
                    rel_path
                );
                let resp = Self::http_client().delete(&url).send().await.map_err(|e| {
                    warn!(error = %e, "proxy delete failed");
                    CloudErrorKind::NetworkUnavailable
                })?;
                if !resp.status().is_success() {
                    warn!(
                        status = %resp.status(),
                        storage = %storage_name,
                        path = %rel_path,
                        "proxy delete returned error"
                    );
                    return Err(CloudErrorKind::NotInSync);
                }
                debug!(storage = %storage_name, path = %rel_path, "deleted via proxy");
            }
        }
        Ok(())
    }

    /// Rename/move a file within a storage on the mount (or proxy to remote).
    async fn do_rename_subpath(
        &self,
        storage_name: &str,
        old_rel: &str,
        new_rel: &str,
    ) -> CResult<()> {
        if let Some(local) = StorageRoute::find_local(storage_name, &self.volumes).await {
            let src = local.mount_path.join(old_rel);
            let dst = local.mount_path.join(new_rel);

            if !src.exists() {
                // Source doesn't exist on mount (placeholder-only) — nothing to move
                return Ok(());
            }

            if let Some(parent) = dst.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            tokio::fs::rename(&src, &dst).await.map_err(|e| {
                warn!(
                    storage = %storage_name,
                    src = %old_rel,
                    dst = %new_rel,
                    error = %e,
                    "rename: fs::rename failed"
                );
                CloudErrorKind::NotInSync
            })?;

            debug!(
                storage = %storage_name,
                src = %old_rel,
                dst = %new_rel,
                "renamed on mount"
            );
        }
        // Remote-only storages: the rename in the sync root is cosmetic
        // (placeholder only).  Next fetch_placeholders will restore the
        // original names.  Cross-stone rename requires API support.
        Ok(())
    }

    /// Rename a top-level storage folder (replica set name) via Explorer.
    ///
    /// Updates `replica_set_name` on both the in-memory volume and the on-disk
    /// manifest.  The individual device name (`mgmt.name`) is unchanged.
    async fn do_rename_storage(&self, old_name: &str, new_name: &str) -> CResult<()> {
        // Verify the old name exists locally (find_local matches on replica_set_name)
        if StorageRoute::find_local(old_name, &self.volumes).await.is_none() {
            warn!(
                old = %old_name,
                new = %new_name,
                "rename rejected: storage not found locally"
            );
            return Err(CloudErrorKind::NotInSync);
        }

        // Update replica_set_name on all local volumes in this replica set
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

        // Update the on-disk manifest for each volume in the replica set
        for mp in &mount_paths {
            if let Err(e) = crate::api::v1::storage::update_manifest_replica_set_name(mp, new_name).await {
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

        info!(old = %old_name, new = %new_name, volumes = mount_paths.len(), "replica set renamed via Explorer");
        Ok(())
    }

    // ========================================================================
    // Directory listing helpers
    // ========================================================================

    /// List a local directory.
    async fn list_local_dir(dir_path: &std::path::Path) -> CResult<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(dir_path).await.map_err(|e| {
            warn!(path = %dir_path.display(), error = %e, "read_dir failed");
            CloudErrorKind::NotInSync
        })?;

        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata().await.ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            entries.push(DirEntry { name, is_dir, size });
        }

        Ok(entries)
    }

    /// List a remote directory via the garden storage API.
    async fn list_remote_dir(
        endpoint: &str,
        storage_name: &str,
        rel_path: &str,
    ) -> CResult<Vec<DirEntry>> {
        let path_segment = if rel_path.is_empty() { "" } else { rel_path };
        let url = format!(
            "{}/api/v1/garden/storage/{}/files/{}",
            endpoint.trim_end_matches('/'),
            storage_name,
            path_segment
        );

        let resp = Self::http_client().get(&url).send().await.map_err(|e| {
            warn!(error = %e, url = %url, "remote list failed");
            CloudErrorKind::NetworkUnavailable
        })?;

        let status = resp.status();
        if !status.is_success() {
            warn!(url = %url, status = %status, "remote storage returned error for dir listing");
            return Ok(vec![]);
        }

        let body = resp.text().await.unwrap_or_default();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let entries_json = json
            .get("data")
            .and_then(|d| d.get("entries"))
            .and_then(|e| e.as_array());

        let mut entries = Vec::new();
        if let Some(arr) = entries_json {
            for item in arr {
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let is_dir = item.get("type").and_then(|t| t.as_str()) == Some("dir");
                let size = item.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                if !name.is_empty() {
                    entries.push(DirEntry { name, is_dir, size });
                }
            }
        } else {
            warn!(
                url = %url,
                body_preview = %body.chars().take(200).collect::<String>(),
                "could not parse remote dir listing"
            );
        }

        Ok(entries)
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

        debug!(storage = %storage_name, path = %rel_path, "fetch_data");
        self.do_fetch_data(&storage_name, &rel_path, &ticket, &info).await
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
            let entries = self.list_storages().await;

            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            info!(
                storages = ?names,
                sync_root = %self.sync_root_path.display(),
                "fetch_placeholders: enumerating storages for sync root"
            );

            let mut phs: Vec<PlaceholderFile> = entries
                .iter()
                .map(|e| placeholders::build_placeholder(&e.name, true, 0))
                .collect();

            if phs.is_empty() {
                debug!("no storages known yet, returning empty placeholder list");
                ticket.pass_with_placeholder(&mut phs).map_err(|e| {
                    warn!(error = %e, "empty placeholder list rejected");
                    CloudErrorKind::NotInSync
                })?;
                return Ok(());
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
        self.do_fetch_placeholders(&storage_name, &rel_path, &ticket).await
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

        // Propagate delete to the storage mount (or proxy to remote Primary)
        self.do_delete(&storage_name, &rel_path, delete_info.is_directory()).await?;

        // Approve the deletion in the sync root
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

        let (old_storage, old_rel) = match self.resolve_path(&source_path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        if old_storage.is_empty() {
            return Err(CloudErrorKind::NotSupported);
        }

        // Move OUT of sync root (e.g. Explorer "Delete" → Recycle Bin).
        // Treat as a delete: propagate removal to storage, then approve.
        if !rename_info.target_in_scope() {
            if !old_rel.is_empty() {
                if let Err(e) = self
                    .do_delete(&old_storage, &old_rel, rename_info.is_directory())
                    .await
                {
                    warn!(
                        storage = %old_storage,
                        path = %old_rel,
                        error = ?e,
                        "rename-out: propagation failed, approving anyway"
                    );
                }
            }
            ticket.pass().map_err(|e| {
                warn!(error = %e, "rename ticket.pass() failed");
                CloudErrorKind::NotInSync
            })?;
            info!(
                storage = %old_storage,
                path = %old_rel,
                "file moved out of sync root (delete)"
            );
            return Ok(());
        }

        let (new_storage, new_rel) = match self.resolve_path(&target_path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        // Top-level storage rename (replica set name change)
        if old_rel.is_empty() && new_rel.is_empty() && old_storage != new_storage {
            self.do_rename_storage(&old_storage, &new_storage).await?;
            ticket.pass().map_err(|e| {
                warn!(error = %e, "rename ticket.pass() failed");
                CloudErrorKind::NotInSync
            })?;
            return Ok(());
        }

        // Cross-storage move — not supported
        if old_storage != new_storage {
            warn!(
                src_storage = %old_storage,
                dst_storage = %new_storage,
                "rename rejected: cross-storage moves not supported"
            );
            return Err(CloudErrorKind::NotSupported);
        }

        // Sub-path rename within a storage — propagate to mount
        if !old_rel.is_empty() && !new_rel.is_empty() {
            self.do_rename_subpath(&old_storage, &old_rel, &new_rel).await?;
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
