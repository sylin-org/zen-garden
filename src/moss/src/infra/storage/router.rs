//! Unified storage dispatch — resolve once, operate transparently
//! (STORAGE-0015)
//!
//! `StorageRouter` wraps a `StorageRoute` decision and dispatches
//! operations through `ContentStore` (local) or HTTP (remote).
//! Callers never match on Local vs Proxy — they call operations.
//!
//! ## Responsibilities
//!
//! - **Dispatch**: execute file operations on the correct storage
//! - **Cross-storage**: `transfer` / `transfer_tree` compose two routers
//! - **Ingest**: `ingest` reads from arbitrary paths, writes via router
//!
//! ## Non-responsibilities
//!
//! - Resolution logic → `StorageRoute` (domain)
//! - Business policy → `CloudDrive` (domain)
//! - CfApi translation → `provider.rs` (infra)
//! - HTTP response formatting → API handlers

use std::path::Path;

use anyhow::{bail, Context, Result};
use tracing::{debug, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::domain::storage_service::{LocalStorage, ProxyTarget, StorageRoute};

// ============================================================================
// Public types
// ============================================================================

/// Entry in a directory listing.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// File metadata.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// StorageRouter
// ============================================================================

/// Resolved handle to a storage — local or remote.
///
/// Callers never match on Local vs Proxy. They call operations directly.
/// Local → ContentStore.  Remote → HTTP to existing REST endpoints.
pub struct StorageRouter {
    inner: RouterInner,
    storage_name: String,
}

enum RouterInner {
    Local(LocalStorage),
    Remote(ProxyTarget),
}

impl StorageRouter {
    /// Wrap a resolved route.
    pub fn new(route: StorageRoute, storage_name: impl Into<String>) -> Self {
        let storage_name = storage_name.into();
        let inner = match route {
            StorageRoute::Local(local) => RouterInner::Local(local),
            StorageRoute::Proxy(proxy) => RouterInner::Remote(proxy),
        };
        Self { inner, storage_name }
    }

    /// Resolve for **read** operations and wrap.
    pub async fn for_read(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<Self> {
        let route = StorageRoute::for_read(name, volumes, registry, stone_id).await?;
        Ok(Self::new(route, name))
    }

    /// Resolve for **write** operations and wrap.
    pub async fn for_write(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<Self> {
        let route = StorageRoute::for_write(name, volumes, registry, stone_id).await?;
        Ok(Self::new(route, name))
    }

    pub fn storage_name(&self) -> &str {
        &self.storage_name
    }

    pub fn is_local(&self) -> bool {
        matches!(self.inner, RouterInner::Local(_))
    }

    /// Get the local mount path (only for local routes).
    pub fn mount_path(&self) -> Option<&std::path::Path> {
        match &self.inner {
            RouterInner::Local(local) => Some(&local.mount_path),
            RouterInner::Remote(_) => None,
        }
    }

    // ====================================================================
    // File operations
    // ====================================================================

    /// Read a file.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.read_file(path).await
            }
            RouterInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .get(&url)
                    .send()
                    .await
                    .with_context(|| format!("GET {url}"))?;

                if !resp.status().is_success() {
                    bail!("GET {url} returned {}", resp.status());
                }
                Ok(resp.bytes().await?.to_vec())
            }
        }
    }

    /// Write a file (creates parent dirs automatically).
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.write_file(path, data).await
            }
            RouterInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .put(&url)
                    .body(data.to_vec())
                    .send()
                    .await
                    .with_context(|| format!("PUT {url}"))?;

                if !resp.status().is_success() {
                    bail!("PUT {url} returned {}", resp.status());
                }
                Ok(())
            }
        }
    }

    /// Delete a file.
    pub async fn delete_file(&self, path: &str) -> Result<()> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.delete_file(path).await
            }
            RouterInner::Remote(target) => self.remote_delete(target, path).await,
        }
    }

    /// Delete a directory tree.
    pub async fn delete_dir(&self, path: &str) -> Result<()> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.delete_dir(path).await
            }
            RouterInner::Remote(target) => self.remote_delete(target, path).await,
        }
    }

    /// List entries in a directory.
    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                let raw = store.list_dir(path).await?;
                Ok(raw
                    .into_iter()
                    .map(|(name, is_dir, size, modified)| FileEntry {
                        name,
                        is_dir,
                        size,
                        modified,
                    })
                    .collect())
            }
            RouterInner::Remote(target) => self.remote_list(target, path).await,
        }
    }

    /// Rename/move within the same storage.
    ///
    /// For remote routes this is a composed operation: read → write → delete.
    pub async fn rename(&self, old: &str, new: &str) -> Result<()> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.rename_path(old, new).await
            }
            RouterInner::Remote(_) => {
                let data = self.read(old).await?;
                self.write(new, &data).await?;
                self.delete_file(old).await?;
                Ok(())
            }
        }
    }

    /// Create a directory.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                store.mkdir(path).await
            }
            RouterInner::Remote(_) => {
                // Remote directories are created implicitly by PUT.
                // No explicit mkdir endpoint needed.
                Ok(())
            }
        }
    }

    /// Get file metadata.
    pub async fn metadata(&self, path: &str) -> Result<FileMeta> {
        match &self.inner {
            RouterInner::Local(local) => {
                let store = local.content_store();
                let meta = store.file_metadata(path).await?;
                Ok(FileMeta {
                    size: meta.len(),
                    is_dir: meta.is_dir(),
                    modified: meta
                        .modified()
                        .ok()
                        .map(chrono::DateTime::<chrono::Utc>::from),
                })
            }
            RouterInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .head(&url)
                    .send()
                    .await
                    .with_context(|| format!("HEAD {url}"))?;

                if !resp.status().is_success() {
                    bail!("HEAD {url} returned {}", resp.status());
                }

                let size = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                let modified = resp
                    .headers()
                    .get(reqwest::header::LAST_MODIFIED)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));

                Ok(FileMeta {
                    size,
                    is_dir: false,
                    modified,
                })
            }
        }
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> Result<bool> {
        match &self.inner {
            RouterInner::Local(local) => {
                let full = local.mount_path.join(path);
                Ok(full.exists())
            }
            RouterInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client().head(&url).send().await;
                Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
            }
        }
    }

    // ====================================================================
    // Internal helpers
    // ====================================================================

    fn file_url(&self, target: &ProxyTarget, path: &str) -> String {
        format!(
            "{}/api/v1/garden/storage/{}/files/{}",
            target.endpoint.trim_end_matches('/'),
            self.storage_name,
            path,
        )
    }

    async fn remote_delete(&self, target: &ProxyTarget, path: &str) -> Result<()> {
        let url = self.file_url(target, path);
        let resp = http_client()
            .delete(&url)
            .send()
            .await
            .with_context(|| format!("DELETE {url}"))?;

        if !resp.status().is_success() {
            bail!("DELETE {url} returned {}", resp.status());
        }
        Ok(())
    }

    async fn remote_list(&self, target: &ProxyTarget, path: &str) -> Result<Vec<FileEntry>> {
        let url = self.file_url(target, path);
        let resp = http_client()
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url} (list)"))?;

        if !resp.status().is_success() {
            bail!("GET {url} (list) returned {}", resp.status());
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
                let modified = item
                    .get("modified")
                    .and_then(|m| m.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                if !name.is_empty() {
                    entries.push(FileEntry { name, is_dir, size, modified });
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
// Cross-storage free functions
// ============================================================================

/// Copy a single file between two storages.
pub async fn transfer(
    src: &StorageRouter,
    src_path: &str,
    dst: &StorageRouter,
    dst_path: &str,
) -> Result<()> {
    let data = src.read(src_path).await?;
    dst.write(dst_path, &data).await?;
    debug!(
        src_storage = %src.storage_name(),
        src_path,
        dst_storage = %dst.storage_name(),
        dst_path,
        bytes = data.len(),
        "transferred file"
    );
    Ok(())
}

/// Recursively copy a directory tree between two storages.
pub async fn transfer_tree(
    src: &StorageRouter,
    src_path: &str,
    dst: &StorageRouter,
    dst_path: &str,
) -> Result<()> {
    dst.mkdir(dst_path).await?;
    let entries = src.list(src_path).await?;

    for entry in &entries {
        let child_src = if src_path.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", src_path, entry.name)
        };
        let child_dst = if dst_path.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", dst_path, entry.name)
        };

        if entry.is_dir {
            Box::pin(transfer_tree(src, &child_src, dst, &child_dst)).await?;
        } else {
            transfer(src, &child_src, dst, &child_dst).await?;
        }
    }

    Ok(())
}

/// Ingest a file or directory from an arbitrary filesystem path into a storage.
///
/// Used for drag-from-outside-sync-root and stray root item recovery.
pub async fn ingest(
    source: &Path,
    dst: &StorageRouter,
    dst_path: &str,
    is_dir: bool,
) -> Result<()> {
    if is_dir {
        ingest_tree(source, dst, dst_path).await?;
    } else {
        let data = tokio::fs::read(source)
            .await
            .with_context(|| format!("read {}", source.display()))?;
        dst.write(dst_path, &data).await?;
    }

    debug!(
        source = %source.display(),
        storage = %dst.storage_name(),
        path = %dst_path,
        "ingested from outside"
    );
    Ok(())
}

/// Recursively ingest a directory tree from the filesystem into a storage.
async fn ingest_tree(source: &Path, dst: &StorageRouter, dst_path: &str) -> Result<()> {
    dst.mkdir(dst_path).await?;

    let mut rd = tokio::fs::read_dir(source)
        .await
        .with_context(|| format!("read_dir {}", source.display()))?;

    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let child_src = entry.path();
        let child_dst = if dst_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", dst_path, name)
        };

        let ft = entry.file_type().await?;
        if ft.is_dir() {
            Box::pin(ingest_tree(&child_src, dst, &child_dst)).await?;
        } else {
            let data = tokio::fs::read(&child_src)
                .await
                .with_context(|| format!("read {}", child_src.display()))?;
            dst.write(&child_dst, &data).await?;
        }
    }

    Ok(())
}

// ============================================================================
// Shared HTTP client
// ============================================================================

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}
