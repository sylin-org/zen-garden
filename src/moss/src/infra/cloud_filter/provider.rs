//! Cloud Filter provider — implements `Filter` for Windows CfApi callbacks
//!
//! Each callback delegates to the Moss storage API (local or proxied)
//! via the same `StorageService` used by REST and WebDAV handlers.

use std::path::PathBuf;
use std::sync::Arc;

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{info, ticket, Filter, Request};
use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::utility::WriteAt;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::managed_storage::ManagedStorages;
use crate::domain::storage_service::StorageRoute;
use garden_common::storage::StorageTick;

/// Shared state needed by Cloud Filter callbacks.
///
/// All fields are `Arc`-wrapped so the provider is `Send + Sync + 'static`.
/// Constructed once at startup, lives as long as the `Connection`.
pub struct ZenGardenProvider {
    /// Managed storages (local seed banks).
    pub(crate) managed_storages: ManagedStorages,
    /// Garden registry (remote storage beacons).
    pub(crate) registry: GardenRegistry,
    /// This stone's ID.
    pub(crate) stone_id: String,
    /// Storage tick sender (for changelog notifications on writes).
    pub(crate) tick_tx: tokio::sync::broadcast::Sender<StorageTick>,
    /// Sync root directory on the local filesystem.
    pub(crate) sync_root_path: PathBuf,
    /// HTTP endpoint of this stone (for push-back writes via API).
    #[allow(dead_code)]
    pub(crate) local_endpoint: Arc<RwLock<String>>,
}

impl ZenGardenProvider {
    /// Build a `StorageService` from our shared state.
    fn storage_service(&self) -> crate::domain::StorageService<'_> {
        crate::domain::StorageService::new(
            &self.managed_storages,
            &self.registry,
            &self.stone_id,
            Some(&self.tick_tx),
        )
    }

    /// Resolve the storage name and relative path from a Cloud Filter request path.
    ///
    /// The sync root layout is: `{sync_root}/{storage_name}/{relative_path}`.
    /// Returns `(storage_name, relative_path)`.
    fn resolve_path(&self, request_path: &std::path::Path) -> Option<(String, String)> {
        let rel = request_path.strip_prefix(&self.sync_root_path).ok()?;
        let mut components = rel.components();
        let storage_name = components.next()?.as_os_str().to_string_lossy().to_string();
        let remainder: PathBuf = components.collect();
        let rel_path = remainder.to_string_lossy().replace('\\', "/");
        Some((storage_name, rel_path))
    }

    /// Fetch file content from the storage API and write it to the placeholder.
    async fn do_fetch_data(
        &self,
        storage_name: &str,
        rel_path: &str,
        ticket: &ticket::FetchData,
        info: &info::FetchData,
    ) -> CResult<()> {
        let svc = self.storage_service();
        let route = svc.resolve_read(storage_name).await.map_err(|e| {
            warn!(storage = %storage_name, error = %e, "Cloud Filter: storage not found");
            CloudErrorKind::NotInSync
        })?;

        let data = match route {
            StorageRoute::Local(local) => {
                let full_path = local.mount_path.join(rel_path);
                tokio::fs::read(&full_path).await.map_err(|e| {
                    warn!(path = %full_path.display(), error = %e, "Cloud Filter: local read failed");
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
                let client = reqwest::Client::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .unwrap_or_default();

                let resp = client.get(&url).send().await.map_err(|e| {
                    warn!(error = %e, "Cloud Filter: proxy fetch failed");
                    CloudErrorKind::NetworkUnavailable
                })?;

                resp.bytes().await.map_err(|e| {
                    warn!(error = %e, "Cloud Filter: proxy read body failed");
                    CloudErrorKind::NetworkUnavailable
                })?.to_vec()
            }
        };

        let range = info.required_file_range();
        let start = range.start as usize;
        let end = std::cmp::min(range.end as usize, data.len());
        if start < end {
            ticket
                .write_at(&data[start..end], range.start)
                .map_err(|_| CloudErrorKind::NotInSync)?;
        }

        debug!(storage = %storage_name, path = %rel_path, "Cloud Filter: fetched data");
        Ok(())
    }

    /// Populate a directory placeholder with child entries.
    async fn do_fetch_placeholders(
        &self,
        storage_name: &str,
        rel_path: &str,
        ticket: &ticket::FetchPlaceholders,
    ) -> CResult<()> {
        let svc = self.storage_service();
        let route = svc.resolve_read(storage_name).await.map_err(|e| {
            warn!(storage = %storage_name, error = %e, "Cloud Filter: storage not found");
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

        let mut placeholders: Vec<PlaceholderFile> = entries
            .iter()
            .filter(|e| e.name != ".zen-garden" && e.name != "Zen Garden")
            .map(|e| {
                let meta = if e.is_dir {
                    Metadata::directory()
                } else {
                    Metadata::file().size(e.size)
                };
                PlaceholderFile::new(&e.name).metadata(meta).mark_in_sync()
            })
            .collect();

        ticket
            .pass_with_placeholder(&mut placeholders)
            .map_err(|_| CloudErrorKind::NotInSync)?;
        debug!(
            storage = %storage_name,
            path = %rel_path,
            count = placeholders.len(),
            "Cloud Filter: populated placeholders"
        );
        Ok(())
    }

    /// List a local directory, returning simplified entries.
    async fn list_local_dir(dir_path: &std::path::Path) -> CResult<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(dir_path).await.map_err(|e| {
            warn!(error = %e, "Cloud Filter: read_dir failed");
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

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        let resp = client.get(&url).send().await.map_err(|e| {
            warn!(error = %e, "Cloud Filter: remote list failed");
            CloudErrorKind::NetworkUnavailable
        })?;

        let body = resp.text().await.unwrap_or_default();

        // Parse the API response: { "data": { "entries": [...] } }
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
        }

        Ok(entries)
    }
}

/// Simplified directory entry for placeholder creation.
struct DirEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

// ============================================================================
// Filter trait implementation (async)
// ============================================================================

impl Filter for ZenGardenProvider {
    /// Hydrate a placeholder file with remote content.
    async fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let path = request.path();
        let Some((storage_name, rel_path)) = self.resolve_path(&path) else {
            warn!(path = ?path, "Cloud Filter: could not resolve path");
            return Err(CloudErrorKind::NotUnderSyncRoot);
        };

        if rel_path.is_empty() {
            return Ok(());
        }

        self.do_fetch_data(&storage_name, &rel_path, &ticket, &info)
            .await
    }

    /// Populate a directory with placeholder children.
    async fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        _info: info::FetchPlaceholders,
    ) -> CResult<()> {
        let path = request.path();
        let Some((storage_name, rel_path)) = self.resolve_path(&path) else {
            warn!(path = ?path, "Cloud Filter: could not resolve path for placeholders");
            return Err(CloudErrorKind::NotUnderSyncRoot);
        };

        self.do_fetch_placeholders(&storage_name, &rel_path, &ticket)
            .await
    }
}
