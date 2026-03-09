//! Cloud Filter provider — implements `Filter` for Windows CfApi callbacks
//! (STORAGE-0012)
//!
//! Each callback delegates to the Moss storage API (local or proxied)
//! via `StorageService` (domain).  Pure adapter — no business logic.

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
    pub(crate) tick_tx: tokio::sync::broadcast::Sender<StorageTick>,
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

impl ZenGardenProvider {
    /// Build a `StorageService` from our shared state.
    fn storage_service(&self) -> crate::domain::StorageService<'_> {
        crate::domain::StorageService::new(
            &self.volumes,
            &self.registry,
            &self.stone_id,
            Some(&self.tick_tx),
        )
    }

    /// Resolve the storage name and relative path from a Cloud Filter request.
    ///
    /// Layout: `{sync_root}/{storage_name}/{relative_path}`.
    fn resolve_path(&self, request_path: &std::path::Path) -> Option<(String, String)> {
        let rel = request_path.strip_prefix(&self.sync_root_path).ok()?;
        let mut components = rel.components();

        let storage_name = match components.next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => return Some((String::new(), String::new())),
        };

        let remainder: PathBuf = components.collect();
        let rel_path = remainder.to_string_lossy().replace('\\', "/");
        Some((storage_name, rel_path))
    }

    /// List all known storages (local + remote) as directory entries.
    pub(crate) async fn list_storages(&self) -> Vec<DirEntry> {
        let mut names = std::collections::HashSet::new();

        // Local managed storages
        {
            let map = self.volumes.read().await;
            for vol in map.values() {
                if let Some(ref mgmt) = vol.management {
                    debug!(storage = %mgmt.name, "found local managed storage");
                    names.insert(mgmt.name.clone());
                }
            }
        }

        // Remote storages from registry (name lives in `fqid`)
        {
            let reg = self.registry.read().await;
            for entry in reg.storage_entries() {
                let name = &entry.tool.fqid;
                if !name.is_empty() {
                    debug!(storage = %name, "found remote storage in registry");
                    names.insert(name.clone());
                }
            }
        }

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

    /// Fetch file content and write it to the placeholder.
    async fn do_fetch_data(
        &self,
        storage_name: &str,
        rel_path: &str,
        ticket: &ticket::FetchData,
        info: &info::FetchData,
    ) -> CResult<()> {
        let svc = self.storage_service();
        let route = svc.resolve_read(storage_name).await.map_err(|e| {
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
                let client = reqwest::Client::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .unwrap_or_default();

                let resp = client.get(&url).send().await.map_err(|e| {
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
        let svc = self.storage_service();
        let route = svc.resolve_read(storage_name).await.map_err(|e| {
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
            .filter(|e| e.name != ".zen-garden" && e.name != "Zen Garden")
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

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        let resp = client.get(&url).send().await.map_err(|e| {
            warn!(error = %e, url = %url, "remote list failed");
            CloudErrorKind::NetworkUnavailable
        })?;

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
// Filter trait implementation (async)
// ============================================================================

impl Filter for ZenGardenProvider {
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

        // Directories are not fetchable data
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

            // Log per-entry results for diagnostics
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
}
