//! Unified storage dispatch — resolve once, operate transparently
//! (STORAGE-0015)
//!
//! `StorageHandle` wraps a `StorageRoute` decision and dispatches
//! operations through `ContentStore` (local) or HTTP (remote).
//! Callers never match on Local vs Proxy — they call operations.
//!
//! `StorageResolver` centralises the 4-arg resolution pattern so
//! handlers resolve once and receive a ready handle.
//!
//! ## Responsibilities
//!
//! - **Resolution**: `StorageResolver` resolves name → handle
//! - **Dispatch**: execute file operations on the correct storage
//! - **Cross-storage**: `transfer` / `transfer_tree` compose two handles
//! - **Ingest**: `ingest` reads from arbitrary paths, writes via handle
//!
//! ## Non-responsibilities
//!
//! - Route decision logic → `StorageRoute` (domain)
//! - Business policy → `CloudDrive` (domain)
//! - CfApi translation → `provider.rs` (infra)
//! - HTTP response formatting → API handlers

use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::sync::broadcast;
use tracing::{debug, error};

use crate::domain::storage::Volumes;
use crate::domain::storage_service::{LocalStorage, ProxyTarget, StorageRoute};
use crate::domain::tool::registry::GardenRegistry;
use garden_common::storage::StorageTick;

use super::{ContentStore, ObjectStore};

// ============================================================================
// Store construction (infra extension for domain LocalStorage)
// ============================================================================

impl LocalStorage {
    /// Build a read-only `ContentStore` for this local storage.
    pub fn content_store(&self) -> ContentStore {
        ContentStore::new(self.mount_path.clone(), None)
    }

    /// Build a `ContentStore` with changelog notifications (for writes).
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
// Typed error (A11a — replaces string-based error detection)
// ============================================================================

/// Structured error for storage router operations.
///
/// Handlers match on these variants to produce correct HTTP status codes
/// without string inspection (code standard #10).
#[derive(Debug)]
pub enum RouterError {
    /// Resource does not exist (→ 404).
    NotFound(String),
    /// Any other failure (→ 500 or 503 depending on context).
    Other(anyhow::Error),
}

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RouterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for RouterError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

/// Classify an `anyhow::Error` as `NotFound` or `Other` based on the I/O error kind.
fn classify_error(e: anyhow::Error, context: &str) -> RouterError {
    // Walk the error chain for an io::ErrorKind::NotFound
    if let Some(io_err) = e.chain().find_map(|c| c.downcast_ref::<std::io::Error>())
        && io_err.kind() == std::io::ErrorKind::NotFound
    {
        return RouterError::NotFound(context.to_string());
    }
    RouterError::Other(e)
}

/// Classify an HTTP response status into `RouterError` when it indicates failure.
///
/// Returns `Ok(())` for success statuses, `Err(NotFound)` for 404,
/// and `Err(Other)` for all other error statuses.
fn classify_http_response(
    status: reqwest::StatusCode,
    url: &str,
    path: &str,
) -> Result<(), RouterError> {
    if status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT {
        return Ok(());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(RouterError::NotFound(path.to_string()));
    }
    Err(RouterError::Other(anyhow::anyhow!(
        "{url} returned {status}"
    )))
}

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
// StorageResolver — centralised 4-arg resolution
// ============================================================================

/// Resolves a storage name into a `StorageHandle`, eliminating repeated
/// `(volumes, registry, stone_id)` argument tuples in every handler.
pub struct StorageResolver<'a> {
    pub volumes: &'a Volumes,
    pub registry: &'a GardenRegistry,
    pub stone_id: &'a str,
    pub tick: Option<broadcast::Sender<StorageTick>>,
}

impl<'a> StorageResolver<'a> {
    /// Resolve for **read** operations.
    pub async fn for_read(&self, name: &str) -> Result<StorageHandle> {
        let route =
            StorageRoute::for_read(name, self.volumes, self.registry, self.stone_id).await?;
        Ok(StorageHandle::new(route, name, self.tick.clone()))
    }

    /// Resolve for **write** operations.
    pub async fn for_write(&self, name: &str) -> Result<StorageHandle> {
        let route =
            StorageRoute::for_write(name, self.volumes, self.registry, self.stone_id).await?;
        Ok(StorageHandle::new(route, name, self.tick.clone()))
    }
}

// ============================================================================
// StorageHandle
// ============================================================================

/// Resolved handle to a storage — local or remote.
///
/// Callers never match on Local vs Proxy. They call operations directly.
/// Local → ContentStore (with optional tick notifications).
/// Remote → HTTP to existing REST endpoints.
pub struct StorageHandle {
    inner: HandleInner,
    storage_name: String,
    tick: Option<broadcast::Sender<StorageTick>>,
}

enum HandleInner {
    Local(LocalStorage),
    Remote(ProxyTarget),
}

impl StorageHandle {
    /// Wrap a resolved route.
    pub fn new(
        route: StorageRoute,
        storage_name: impl Into<String>,
        tick: Option<broadcast::Sender<StorageTick>>,
    ) -> Self {
        let storage_name = storage_name.into();
        let inner = match route {
            StorageRoute::Local(local) => HandleInner::Local(local),
            StorageRoute::Proxy(proxy) => HandleInner::Remote(proxy),
        };
        Self {
            inner,
            storage_name,
            tick,
        }
    }

    /// Resolve for **read** operations and wrap (convenience without resolver).
    pub async fn for_read(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<Self> {
        let route = StorageRoute::for_read(name, volumes, registry, stone_id).await?;
        Ok(Self::new(route, name, None))
    }

    /// Resolve for **write** operations and wrap (convenience without resolver).
    pub async fn for_write(
        name: &str,
        volumes: &Volumes,
        registry: &GardenRegistry,
        stone_id: &str,
    ) -> Result<Self> {
        let route = StorageRoute::for_write(name, volumes, registry, stone_id).await?;
        Ok(Self::new(route, name, None))
    }

    pub fn storage_name(&self) -> &str {
        &self.storage_name
    }

    /// Get the local volume/device ID (only for local routes).
    pub fn volume_id(&self) -> Option<&str> {
        match &self.inner {
            HandleInner::Local(local) => Some(&local.id),
            HandleInner::Remote(_) => None,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self.inner, HandleInner::Local(_))
    }

    /// Get the local mount path (only for local routes).
    pub fn mount_path(&self) -> Option<&std::path::Path> {
        match &self.inner {
            HandleInner::Local(local) => Some(&local.mount_path),
            HandleInner::Remote(_) => None,
        }
    }

    /// Get the remote endpoint URL (only for proxy routes).
    pub fn remote_endpoint(&self) -> Option<&str> {
        match &self.inner {
            HandleInner::Remote(target) => Some(&target.endpoint),
            HandleInner::Local(_) => None,
        }
    }

    /// Get the proxy target (only for proxy routes).
    pub fn proxy_target(&self) -> Option<&ProxyTarget> {
        match &self.inner {
            HandleInner::Remote(target) => Some(target),
            HandleInner::Local(_) => None,
        }
    }

    /// Build a read-only `ContentStore` for local routes.
    pub fn content_store_for_read(&self) -> Option<super::ContentStore> {
        match &self.inner {
            HandleInner::Local(local) => Some(local.content_store()),
            HandleInner::Remote(_) => None,
        }
    }

    /// Build a tick-aware `ContentStore` for local write routes.
    pub fn content_store_for_write(&self) -> Option<super::ContentStore> {
        match &self.inner {
            HandleInner::Local(local) => Some(local.notifying_content_store(self.tick.as_ref())),
            HandleInner::Remote(_) => None,
        }
    }

    /// Build a read-only `ObjectStore` for local routes.
    pub fn object_store_for_read(&self) -> Option<super::ObjectStore> {
        match &self.inner {
            HandleInner::Local(local) => Some(local.object_store()),
            HandleInner::Remote(_) => None,
        }
    }

    /// Build a tick-aware `ObjectStore` for local write routes.
    pub fn object_store_for_write(&self) -> Option<super::ObjectStore> {
        match &self.inner {
            HandleInner::Local(local) => Some(local.notifying_object_store(self.tick.as_ref())),
            HandleInner::Remote(_) => None,
        }
    }

    // ====================================================================
    // Private: content store with tick awareness (A1 fix)
    // ====================================================================

    /// Build a content store that emits tick notifications on writes when
    /// a tick sender is present.
    fn content_store(&self, local: &LocalStorage) -> super::ContentStore {
        if self.tick.is_some() {
            local.notifying_content_store(self.tick.as_ref())
        } else {
            local.content_store()
        }
    }

    // ====================================================================
    // File operations
    // ====================================================================

    /// Read a file.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>, RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = local.content_store();
                store
                    .read_file(path)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .get(&url)
                    .send()
                    .await
                    .with_context(|| format!("GET {url}"))
                    .map_err(RouterError::Other)?;

                classify_http_response(resp.status(), &url, path)?;
                Ok(resp
                    .bytes()
                    .await
                    .map_err(|e| RouterError::Other(e.into()))?
                    .to_vec())
            }
        }
    }

    /// Read a byte range from a file (A11j — ranged read for CfApi hydration).
    ///
    /// Local: seek-based (no full-file load for unencrypted stores).
    /// Remote: HTTP Range header.
    pub async fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = local.content_store();
                store
                    .read_range(path, offset, length)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => {
                let url = self.file_url(target, path);
                let end = offset + length - 1;
                let resp = http_client()
                    .get(&url)
                    .header(reqwest::header::RANGE, format!("bytes={offset}-{end}"))
                    .send()
                    .await
                    .with_context(|| format!("GET {url} (range)"))
                    .map_err(RouterError::Other)?;

                classify_http_response(resp.status(), &url, path)?;
                Ok(resp
                    .bytes()
                    .await
                    .map_err(|e| RouterError::Other(e.into()))?
                    .to_vec())
            }
        }
    }

    /// Streaming read — returns an async reader (A11j).
    ///
    /// Local unencrypted: streams directly from `tokio::fs::File`.
    /// Local encrypted: decrypts entire file (AEAD), returns `Cursor`.
    /// Remote: streams the `reqwest` response body.
    ///
    /// Callers never branch on encryption — the handle falls back internally.
    pub async fn open_read(
        &self,
        path: &str,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>, RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = local.content_store();
                store
                    .open_read(path)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .get(&url)
                    .send()
                    .await
                    .with_context(|| format!("GET {url}"))
                    .map_err(RouterError::Other)?;

                classify_http_response(resp.status(), &url, path)?;

                // Stream the response body via StreamReader
                use futures_util::StreamExt;
                let byte_stream = resp
                    .bytes_stream()
                    .map(|result| result.map_err(std::io::Error::other));
                let reader = tokio_util::io::StreamReader::new(byte_stream);
                Ok(Box::new(reader))
            }
        }
    }

    /// Plaintext file size (A11j — needed for Content-Length on streaming responses).
    pub async fn file_size(&self, path: &str) -> Result<u64, RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = local.content_store();
                store
                    .file_size(path)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => {
                // HEAD to get Content-Length
                let url = self.file_url(target, path);
                let resp = http_client()
                    .head(&url)
                    .timeout(METADATA_TIMEOUT)
                    .send()
                    .await
                    .with_context(|| format!("HEAD {url}"))
                    .map_err(RouterError::Other)?;

                classify_http_response(resp.status(), &url, path)?;

                let size = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                Ok(size)
            }
        }
    }

    /// Write a file (creates parent dirs automatically).
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                store.write_file(path, data).await
            }
            HandleInner::Remote(target) => {
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
    pub async fn delete_file(&self, path: &str) -> Result<(), RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                store
                    .delete_file(path)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => self.remote_delete(target, path).await,
        }
    }

    /// Delete a directory tree.
    pub async fn delete_dir(&self, path: &str) -> Result<(), RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                store
                    .delete_dir(path)
                    .await
                    .map_err(|e| classify_error(e, path))
            }
            HandleInner::Remote(target) => self.remote_delete(target, path).await,
        }
    }

    /// List entries in a directory.
    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        match &self.inner {
            HandleInner::Local(local) => {
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
            HandleInner::Remote(target) => self.remote_list(target, path).await,
        }
    }

    /// Rename/move within the same storage.
    ///
    /// Local: filesystem rename (handles files and directories).
    /// Remote: composed copy + delete (file: read → write → delete;
    /// directory: transfer_tree → delete_dir).
    pub async fn rename(&self, old: &str, new: &str, is_dir: bool) -> Result<()> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                store.rename_path(old, new).await
            }
            HandleInner::Remote(_) => {
                if is_dir {
                    transfer_tree(self, old, self, new).await?;
                    self.delete_dir(old).await.map_err(anyhow::Error::from)?;
                } else {
                    let data = self.read(old).await.map_err(anyhow::Error::from)?;
                    self.write(new, &data).await?;
                    self.delete_file(old).await.map_err(anyhow::Error::from)?;
                }
                Ok(())
            }
        }
    }

    /// Copy a file within the same storage.
    ///
    /// Local: filesystem copy.  Remote: GET then PUT.
    pub async fn copy_file(&self, src: &str, dst: &str) -> Result<()> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                let full_src = store.mount_root().join(src);
                let full_dst = store.mount_root().join(dst);
                if let Some(parent) = full_dst.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::copy(&full_src, &full_dst)
                    .await
                    .with_context(|| format!("copy {} → {}", src, dst))?;
                Ok(())
            }
            HandleInner::Remote(_) => {
                let data = self.read(src).await.map_err(anyhow::Error::from)?;
                self.write(dst, &data).await
            }
        }
    }

    /// Recursively copy a directory tree within the same storage.
    ///
    /// Local: filesystem walk + copy.  Remote: list + GET/PUT per file.
    pub async fn copy_tree(&self, src: &str, dst: &str) -> Result<()> {
        transfer_tree(self, src, self, dst).await
    }

    /// Create a directory.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = self.content_store(local);
                store.mkdir(path).await
            }
            HandleInner::Remote(_) => {
                // Remote directories are created implicitly by PUT.
                // No explicit mkdir endpoint needed.
                Ok(())
            }
        }
    }

    /// Get file metadata.
    pub async fn metadata(&self, path: &str) -> Result<FileMeta, RouterError> {
        match &self.inner {
            HandleInner::Local(local) => {
                let store = local.content_store();
                let meta = store
                    .file_metadata(path)
                    .await
                    .map_err(|e| classify_error(e, path))?;
                Ok(FileMeta {
                    size: meta.len(),
                    is_dir: meta.is_dir(),
                    modified: meta
                        .modified()
                        .ok()
                        .map(chrono::DateTime::<chrono::Utc>::from),
                })
            }
            HandleInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .head(&url)
                    .timeout(METADATA_TIMEOUT)
                    .send()
                    .await
                    .with_context(|| format!("HEAD {url}"))
                    .map_err(RouterError::Other)?;

                classify_http_response(resp.status(), &url, path)?;

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
            HandleInner::Local(local) => {
                let full = local.mount_path.join(path);
                Ok(full.exists())
            }
            HandleInner::Remote(target) => {
                let url = self.file_url(target, path);
                let resp = http_client()
                    .head(&url)
                    .timeout(METADATA_TIMEOUT)
                    .send()
                    .await;
                Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
            }
        }
    }

    // ====================================================================
    // Internal helpers
    // ====================================================================

    fn file_url(&self, target: &ProxyTarget, path: &str) -> String {
        format!(
            "{}/api/v1/garden/storage/{}/fs/{}",
            target.endpoint.trim_end_matches('/'),
            self.storage_name,
            path,
        )
    }

    /// URL for directory listing — uses the dedicated listing endpoint with
    /// query parameters (S3/GCS model) to avoid the Axum wildcard gap.
    fn list_url(&self, target: &ProxyTarget, path: &str) -> String {
        let base = format!(
            "{}/api/v1/garden/storage/{}/fs",
            target.endpoint.trim_end_matches('/'),
            self.storage_name,
        );
        if path.is_empty() {
            base
        } else {
            format!("{}?path={}", base, urlencoding::encode(path))
        }
    }

    async fn remote_delete(&self, target: &ProxyTarget, path: &str) -> Result<(), RouterError> {
        let url = self.file_url(target, path);
        let resp = http_client()
            .delete(&url)
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("DELETE {url}"))
            .map_err(RouterError::Other)?;

        classify_http_response(resp.status(), &url, path)
    }

    async fn remote_list(&self, target: &ProxyTarget, path: &str) -> Result<Vec<FileEntry>> {
        let url = self.list_url(target, path);
        let resp = http_client()
            .get(&url)
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("GET {url} (list)"))?;

        if !resp.status().is_success() {
            bail!("GET {url} (list) returned {}", resp.status());
        }

        let body = resp.text().await.unwrap_or_default();
        let json: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    url = %url,
                    error = %e,
                    body_preview = %body.chars().take(200).collect::<String>(),
                    "remote dir listing: response is not valid JSON"
                );
                return Ok(Vec::new());
            }
        };

        let Some(data) = json.get("data") else {
            error!(
                url = %url,
                keys = ?json.as_object().map(|o| o.keys().collect::<Vec<_>>()),
                "remote dir listing: missing \"data\" key"
            );
            return Ok(Vec::new());
        };

        let Some(arr) = data.get("entries").and_then(|e| e.as_array()) else {
            error!(
                url = %url,
                keys = ?data.as_object().map(|o| o.keys().collect::<Vec<_>>()),
                "remote dir listing: missing or non-array \"entries\" key"
            );
            return Ok(Vec::new());
        };

        let mut entries = Vec::new();
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
                entries.push(FileEntry {
                    name,
                    is_dir,
                    size,
                    modified,
                });
            }
        }

        Ok(entries)
    }
}

// ============================================================================
// Cross-storage free functions
// ============================================================================

/// Copy a single file between two storages.
///
/// Fast path (A11j Wave 2): when both src and dst are local and the dst
/// is unencrypted, streams via `open_read()` → `write_from_reader()` —
/// no full-file `Vec<u8>` allocation.
///
/// Fallback: buffered `read()` + `write()` (encrypted dst, remote, or mixed).
pub async fn transfer(
    src: &StorageHandle,
    src_path: &str,
    dst: &StorageHandle,
    dst_path: &str,
) -> Result<()> {
    // Fast path: both local, dst unencrypted → streaming copy
    if let (HandleInner::Local(src_local), HandleInner::Local(dst_local)) = (&src.inner, &dst.inner)
    {
        let dst_store = if dst.tick.is_some() {
            dst_local.notifying_content_store(dst.tick.as_ref())
        } else {
            dst_local.content_store()
        };

        if !dst_store.is_encrypted() {
            let src_store = src_local.content_store();
            let mut reader = src_store
                .open_read(src_path)
                .await
                .with_context(|| format!("open_read {src_path}"))?;
            let bytes = dst_store.write_from_reader(dst_path, &mut *reader).await?;
            debug!(
                src_storage = %src.storage_name(),
                src_path,
                dst_storage = %dst.storage_name(),
                dst_path,
                bytes,
                "transferred file (streaming)"
            );
            return Ok(());
        }
    }

    // Fallback: buffered
    let data = src.read(src_path).await.map_err(anyhow::Error::from)?;
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

/// Maximum recursion depth for tree operations (transfer, ingest).
///
/// Prevents unbounded stack growth from symlink loops or adversarial directory structures.
const MAX_TREE_DEPTH: usize = 64;

/// Recursively copy a directory tree between two storages.
pub async fn transfer_tree(
    src: &StorageHandle,
    src_path: &str,
    dst: &StorageHandle,
    dst_path: &str,
) -> Result<()> {
    transfer_tree_inner(src, src_path, dst, dst_path, 0).await
}

async fn transfer_tree_inner(
    src: &StorageHandle,
    src_path: &str,
    dst: &StorageHandle,
    dst_path: &str,
    depth: usize,
) -> Result<()> {
    if depth >= MAX_TREE_DEPTH {
        bail!("transfer_tree: depth limit ({MAX_TREE_DEPTH}) exceeded at {src_path}");
    }

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
            Box::pin(transfer_tree_inner(
                src,
                &child_src,
                dst,
                &child_dst,
                depth + 1,
            ))
            .await?;
        } else {
            transfer(src, &child_src, dst, &child_dst).await?;
        }
    }

    Ok(())
}

/// Ingest a file or directory from an arbitrary filesystem path into a storage.
///
/// Used for drag-from-outside-sync-root and stray root item recovery.
///
/// Fast path (A11j Wave 2): when dst is local and unencrypted, streams
/// the source file via `write_from_reader()` — no full-file buffer.
pub async fn ingest(
    source: &Path,
    dst: &StorageHandle,
    dst_path: &str,
    is_dir: bool,
) -> Result<()> {
    if is_dir {
        ingest_tree(source, dst, dst_path).await?;
    } else {
        ingest_file(source, dst, dst_path).await?;
    }

    debug!(
        source = %source.display(),
        storage = %dst.storage_name(),
        path = %dst_path,
        "ingested from outside"
    );
    Ok(())
}

/// Ingest a single file — streaming when possible.
async fn ingest_file(source: &Path, dst: &StorageHandle, dst_path: &str) -> Result<()> {
    // Fast path: local unencrypted dst → streaming write
    if let HandleInner::Local(local) = &dst.inner {
        let store = if dst.tick.is_some() {
            local.notifying_content_store(dst.tick.as_ref())
        } else {
            local.content_store()
        };
        if !store.is_encrypted() {
            let mut file = tokio::fs::File::open(source)
                .await
                .with_context(|| format!("open {}", source.display()))?;
            store.write_from_reader(dst_path, &mut file).await?;
            return Ok(());
        }
    }

    // Fallback: buffered
    let data = tokio::fs::read(source)
        .await
        .with_context(|| format!("read {}", source.display()))?;
    dst.write(dst_path, &data).await?;
    Ok(())
}

/// Recursively ingest a directory tree from the filesystem into a storage.
async fn ingest_tree(source: &Path, dst: &StorageHandle, dst_path: &str) -> Result<()> {
    ingest_tree_inner(source, dst, dst_path, 0).await
}

async fn ingest_tree_inner(
    source: &Path,
    dst: &StorageHandle,
    dst_path: &str,
    depth: usize,
) -> Result<()> {
    if depth >= MAX_TREE_DEPTH {
        bail!(
            "ingest_tree: depth limit ({MAX_TREE_DEPTH}) exceeded at {}",
            source.display()
        );
    }

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
            Box::pin(ingest_tree_inner(&child_src, dst, &child_dst, depth + 1)).await?;
        } else {
            ingest_file(&child_src, dst, &child_dst).await?;
        }
    }

    Ok(())
}

// ============================================================================
// Shared HTTP client
// ============================================================================

/// Per-call timeout for metadata operations (HEAD, list, exists, mkdir).
///
/// Not applied to data-transfer operations (read, write) which may
/// legitimately take minutes for large files.
const METADATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .danger_accept_invalid_certs(true) // Pond mTLS — see SECURITY-0001
            .build()
            .unwrap_or_default()
    })
}
