//! Pavilion Cloud Filter provider — `SyncFilter` impl backed by `StoneApi`.
//!
//! Mirrors Moss's `ZenGardenProvider` callback structure but routes
//! every I/O operation through the tended stone's REST API instead of
//! local volumes. The user sees `%USERPROFILE%\Zen Garden\` in
//! Explorer; opening a file triggers `fetch_data`, which streams a
//! byte range from `GET /api/v1/garden/storage/{name}/fs/{path}`.
//!
//! ## SyncFilter vs Filter
//!
//! Pavilion implements the **sync** trait so the `Connection` type
//! stays concrete (`Connection<PavilionProvider>`), which lets us hold
//! it in a `static Mutex<Option<…>>` without type erasure. CfApi
//! callbacks fire on Windows threadpool threads outside any tokio
//! context, so each callback drives StoneApi futures via
//! `Handle::block_on` on a runtime handle captured at construction
//! time (Tauri's tokio runtime).
//!
//! ## Callback coverage
//!
//! | Callback              | Status   | Behavior                                              |
//! |-----------------------|----------|-------------------------------------------------------|
//! | `fetch_data`          | Active   | Range read via `read_file_range`                      |
//! | `fetch_placeholders`  | Active   | Sync root: `list()`. Subdir: `list_directory`         |
//! | `delete`              | Active   | Files and directories via `delete_file`               |
//! | `rename`              | Active   | Intra-storage moves via `move_file` (server creates   |
//! |                       |          | the target's parent dir if it only exists locally)    |
//! | `dehydrate`           | Approve  | Free local cache; data is recoverable                 |
//! | `opened` / `closed`   | Logging  | Diagnose corrupt/unsupported placeholders             |
//! | `deleted` / `renamed` | Logging  | Post-completion confirmation                          |
//! | `dehydrated`          | Logging  | Post-dehydration confirmation                         |
//! | `state_changed`       | Logging  | Attribute change notifications                        |

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{Request, SyncFilter, info, ticket};
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::utility::WriteAt;
use garden_common::client::StoneApi;
use tokio::runtime::Handle;
use tracing::{debug, info, warn};

use super::placeholders::{StorageAvailability, build_placeholder, build_storage_dir_placeholder};

/// Cloud Filter provider that delegates all I/O to a tended stone.
///
/// Constructed once at startup with a snapshot of the current tending;
/// re-tending requires a Pavilion restart. The `StoneApi` is shared via
/// `Arc` so callbacks can clone it cheaply when spawning sub-tasks.
/// `rt` captures Tauri's tokio runtime handle so blocking-thread
/// callbacks can drive async StoneApi calls.
pub struct PavilionProvider {
    api: Arc<StoneApi>,
    sync_root_path: PathBuf,
    rt: Handle,
}

impl PavilionProvider {
    /// Construct a provider. Must be called from a context that has a
    /// tokio runtime active (so `Handle::current()` succeeds) — typically
    /// inside a `tauri::async_runtime::spawn` task.
    pub fn new(api: Arc<StoneApi>, sync_root_path: PathBuf) -> Self {
        Self {
            api,
            sync_root_path,
            rt: Handle::current(),
        }
    }

    /// Decompose a Cloud Filter request path into `(storage, relative)`.
    ///
    /// Layout: `{sync_root}/{storage}/{relative}`. Returns `None` when
    /// the path is not under the sync root, `Some(("", ""))` when it
    /// IS the sync root, and `Some((storage, ""))` when it's a storage
    /// root directory.
    fn resolve_path(&self, request_path: &Path) -> Option<(String, String)> {
        let rel = request_path.strip_prefix(&self.sync_root_path).ok()?;
        let mut components = rel.components();
        let storage_name = match components.next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => return Some((String::new(), String::new())),
        };
        let remainder: PathBuf = components.collect();
        Some((storage_name, remainder.to_string_lossy().replace('\\', "/")))
    }
}

// ============================================================================
// SyncFilter trait — Pavilion implements the read path + delete + lifecycle
// ============================================================================

impl SyncFilter for PavilionProvider {
    // ---- Data hydration (download path) ----

    fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let (storage_name, rel_path) = match self.resolve_path(&request.path()) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        if storage_name.is_empty() || rel_path.is_empty() {
            return Ok(());
        }

        let range = info.required_file_range();
        let length = range.end.saturating_sub(range.start);
        if length == 0 {
            return Ok(());
        }

        let api = self.api.clone();
        let storage_owned = storage_name.clone();
        let rel_owned = rel_path.clone();
        let start = range.start;
        let bytes = self
            .rt
            .block_on(async move {
                api.garden()
                    .storage()
                    .read_file_range(&storage_owned, &rel_owned, start, length)
                    .await
            })
            .map_err(|e| {
                warn!(
                    storage = %storage_name,
                    path = %rel_path,
                    offset = start,
                    length,
                    error = %e,
                    "fetch_data: read_file_range failed"
                );
                CloudErrorKind::NotInSync
            })?;

        if !bytes.is_empty() {
            ticket.write_at(&bytes, start).map_err(|e| {
                warn!(error = %e, "fetch_data: write_at failed");
                CloudErrorKind::NotInSync
            })?;
        }

        debug!(
            storage = %storage_name,
            path = %rel_path,
            offset = start,
            length,
            "hydrated file range"
        );
        Ok(())
    }

    fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        _info: info::FetchPlaceholders,
    ) -> CResult<()> {
        let (storage_name, rel_path) = match self.resolve_path(&request.path()) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        // Sync root itself — list known storages as directories
        if storage_name.is_empty() {
            let api = self.api.clone();
            let storages = self
                .rt
                .block_on(async move { api.garden().storage().list().await })
                .map_err(|e| {
                    warn!(error = %e, "fetch_placeholders: garden storage list failed");
                    CloudErrorKind::NotInSync
                })?;

            let names: Vec<&str> = storages.iter().map(|s| s.name.as_str()).collect();
            info!(
                storages = ?names,
                sync_root = %self.sync_root_path.display(),
                "fetch_placeholders: enumerating storages for sync root"
            );

            let mut phs: Vec<PlaceholderFile> = storages
                .iter()
                .map(|s| {
                    let stone_name = s.primary_stone.clone().unwrap_or_else(|| "unknown".into());
                    let avail = StorageAvailability::online(stone_name);
                    build_storage_dir_placeholder(&s.name, &avail)
                })
                .collect();

            let count = phs.len();
            ticket.pass_with_placeholder(&mut phs).map_err(|e| {
                warn!(
                    error = %e,
                    count,
                    storages = ?names,
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

        // Storage subdirectory — list one level
        debug!(storage = %storage_name, path = %rel_path, "fetch_placeholders");

        let api = self.api.clone();
        let storage_owned = storage_name.clone();
        let rel_owned = rel_path.clone();
        let listing = self
            .rt
            .block_on(async move {
                api.garden()
                    .storage()
                    .list_directory(&storage_owned, &rel_owned, Some(1))
                    .await
            })
            .map_err(|e| {
                warn!(
                    storage = %storage_name,
                    path = %rel_path,
                    error = %e,
                    "fetch_placeholders: list_directory failed"
                );
                CloudErrorKind::NotInSync
            })?;

        let mut phs: Vec<PlaceholderFile> = listing
            .entries
            .iter()
            .map(|e| build_placeholder(&e.name, e.is_dir(), e.size.unwrap_or(0)))
            .collect();

        let count = phs.len();
        ticket.pass_with_placeholder(&mut phs).map_err(|e| {
            warn!(
                storage = %storage_name,
                path = %rel_path,
                count,
                error = %e,
                "pass_with_placeholder failed for subdirectory"
            );
            CloudErrorKind::NotInSync
        })?;

        debug!(
            storage = %storage_name,
            path = %rel_path,
            count,
            "populated placeholders"
        );
        Ok(())
    }

    // ---- File handle lifecycle ----

    fn opened(&self, request: Request, info: info::Opened) {
        if info.metadata_corrupt() || info.metadata_unsupported() {
            warn!(
                path = %request.path().display(),
                corrupt = info.metadata_corrupt(),
                unsupported = info.metadata_unsupported(),
                "opened: placeholder metadata issue"
            );
        }
    }

    fn closed(&self, request: Request, info: info::Closed) {
        if info.deleted() {
            debug!(path = %request.path().display(), "closed (deleted)");
        }
    }

    // ---- Dehydration (free local disk cache) ----

    fn dehydrate(
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

    fn dehydrated(&self, request: Request, info: info::Dehydrated) {
        debug!(
            path = %request.path().display(),
            background = info.background(),
            reason = ?info.reason(),
            "dehydrated"
        );
    }

    // ---- Delete ----

    fn delete(
        &self,
        request: Request,
        ticket: ticket::Delete,
        delete_info: info::Delete,
    ) -> CResult<()> {
        let (storage_name, rel_path) = match self.resolve_path(&request.path()) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        if storage_name.is_empty() {
            return Err(CloudErrorKind::NotSupported);
        }

        if rel_path.is_empty() {
            warn!(
                storage = %storage_name,
                "delete rejected: removing a storage requires `rake storage release`"
            );
            return Err(CloudErrorKind::NotSupported);
        }

        let is_dir = delete_info.is_directory();

        let api = self.api.clone();
        let storage_owned = storage_name.clone();
        let rel_owned = rel_path.clone();
        // The garden DELETE handler dispatches on on-disk metadata,
        // so the same call works for files and directories.
        self.rt
            .block_on(async move {
                api.garden()
                    .storage()
                    .delete_file(&storage_owned, &rel_owned)
                    .await
            })
            .map_err(|e| {
                warn!(
                    storage = %storage_name,
                    path = %rel_path,
                    is_dir,
                    error = %e,
                    "delete failed"
                );
                CloudErrorKind::NotInSync
            })?;

        ticket.pass().map_err(|e| {
            warn!(error = %e, "delete ticket.pass() failed");
            CloudErrorKind::NotInSync
        })?;

        info!(
            storage = %storage_name,
            path = %rel_path,
            is_dir,
            "delete approved and propagated to stone"
        );
        Ok(())
    }

    fn deleted(&self, request: Request, _info: info::Deleted) {
        debug!(path = %request.path().display(), "deleted (post-completion)");
    }

    // ---- Rename / Move ----

    fn rename(
        &self,
        request: Request,
        ticket: ticket::Rename,
        rename_info: info::Rename,
    ) -> CResult<()> {
        let source_path = request.path();
        let target_path = rename_info.target_path();

        let (src_storage, src_rel) = match self.resolve_path(&source_path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };
        let (dst_storage, dst_rel) = match self.resolve_path(&target_path) {
            Some(r) => r,
            None => return Err(CloudErrorKind::NotUnderSyncRoot),
        };

        // Cross-storage moves aren't a single-op concept on the
        // server (different replica sets, different mount roots).
        // Cloud Filter usually fires delete+create for those; reject
        // the rename so it falls back to that path.
        if src_storage != dst_storage {
            warn!(
                source = %source_path.display(),
                target = %target_path.display(),
                src_storage = %src_storage,
                dst_storage = %dst_storage,
                "rename rejected: cross-storage moves are not supported"
            );
            return Err(CloudErrorKind::NotSupported);
        }

        // Renaming/moving the storage root itself isn't meaningful —
        // that's the user-facing replica-set name.
        if src_rel.is_empty() || dst_rel.is_empty() {
            warn!(
                storage = %src_storage,
                "rename rejected: storage root renames must go through `rake storage rename`"
            );
            return Err(CloudErrorKind::NotSupported);
        }

        let api = self.api.clone();
        let storage_owned = src_storage.clone();
        let src_owned = src_rel.clone();
        let dst_owned = dst_rel.clone();
        self.rt
            .block_on(async move {
                api.garden()
                    .storage()
                    .move_file(&storage_owned, &src_owned, &dst_owned)
                    .await
            })
            .map_err(|e| {
                warn!(
                    storage = %src_storage,
                    src = %src_rel,
                    dst = %dst_rel,
                    error = %e,
                    "move_file failed"
                );
                CloudErrorKind::NotInSync
            })?;

        ticket.pass().map_err(|e| {
            warn!(error = %e, "rename ticket.pass() failed");
            CloudErrorKind::NotInSync
        })?;

        info!(
            storage = %src_storage,
            src = %src_rel,
            dst = %dst_rel,
            "rename approved and propagated to stone"
        );
        Ok(())
    }

    fn renamed(&self, request: Request, rename_info: info::Renamed) {
        debug!(
            source = %rename_info.source_path().display(),
            target = %request.path().display(),
            "renamed (post-completion)"
        );
    }

    // ---- State changes (attribute monitoring) ----

    fn state_changed(&self, changes: Vec<PathBuf>) {
        debug!(
            count = changes.len(),
            "state_changed: attribute changes detected"
        );
    }
}

// ============================================================================
// Tests
// ============================================================================
//
// `resolve_path` is the kingpin of every callback: every other method
// derives its `(storage, relative)` pair from this function's output.
// Get the decomposition wrong and the provider routes to the wrong
// storage, leaks user paths outside the sync root, or fails to
// dispatch the sync-root listing. Test every branch.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Build a provider rooted at `C:\Users\test\Zen Garden`. The
    /// `StoneApi` is never called in these tests — the URL is dummy.
    fn provider() -> PavilionProvider {
        let api = Arc::new(StoneApi::new(
            reqwest::Client::new(),
            "http://localhost:7185".to_string(),
        ));
        let sync_root = PathBuf::from(r"C:\Users\test\Zen Garden");
        PavilionProvider::new(api, sync_root)
    }

    #[tokio::test]
    async fn resolve_sync_root_itself_returns_empty_storage_and_path() {
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(r"C:\Users\test\Zen Garden"))
            .expect("sync root must resolve");
        assert_eq!(resolved.0, "");
        assert_eq!(resolved.1, "");
    }

    #[tokio::test]
    async fn resolve_storage_root_returns_storage_only() {
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(r"C:\Users\test\Zen Garden\storage"))
            .expect("storage root must resolve");
        assert_eq!(resolved.0, "storage");
        assert_eq!(resolved.1, "");
    }

    #[tokio::test]
    async fn resolve_top_level_file_returns_storage_and_relative() {
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(
                r"C:\Users\test\Zen Garden\storage\readme.txt",
            ))
            .expect("top-level file must resolve");
        assert_eq!(resolved.0, "storage");
        assert_eq!(resolved.1, "readme.txt");
    }

    #[tokio::test]
    async fn resolve_nested_path_normalizes_backslashes_to_forward_slashes() {
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(
                r"C:\Users\test\Zen Garden\storage\photos\vacation\beach.jpg",
            ))
            .expect("nested path must resolve");
        assert_eq!(resolved.0, "storage");
        // Cloud Filter hands us Windows paths with backslashes; the
        // garden API speaks forward slashes. The conversion happens
        // here, exactly once.
        assert_eq!(resolved.1, "photos/vacation/beach.jpg");
        assert!(!resolved.1.contains('\\'), "no leftover backslashes");
    }

    #[tokio::test]
    async fn resolve_path_outside_sync_root_returns_none() {
        let p = provider();
        let resolved = p.resolve_path(&PathBuf::from(r"C:\Users\test\OtherFolder\x.txt"));
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn resolve_path_on_different_drive_returns_none() {
        let p = provider();
        let resolved = p.resolve_path(&PathBuf::from(r"D:\Zen Garden\storage\x.txt"));
        assert!(
            resolved.is_none(),
            "different drive cannot be inside the sync root"
        );
    }

    #[tokio::test]
    async fn resolve_storage_with_special_characters_preserves_name() {
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(
                r"C:\Users\test\Zen Garden\my storage\readme.txt",
            ))
            .expect("space-named storage must resolve");
        assert_eq!(resolved.0, "my storage");
        assert_eq!(resolved.1, "readme.txt");
    }

    #[tokio::test]
    async fn resolve_replica_set_with_double_colon_preserves_full_name() {
        // `storage::personal` is the canonical FQN for a named replica
        // set. The double colon must survive path decomposition — it
        // is part of the storage name, not a path separator.
        let p = provider();
        let resolved = p
            .resolve_path(&PathBuf::from(
                r"C:\Users\test\Zen Garden\storage::personal\photo.jpg",
            ))
            .expect("named replica set must resolve");
        assert_eq!(resolved.0, "storage::personal");
        assert_eq!(resolved.1, "photo.jpg");
    }
}
