//! Filesystem-watcher-driven upload (PAVILION-0002 §"Add Cloud
//! Filter upload to M1 critical path").
//!
//! Cloud Filter's [`SyncFilter::state_changed`][trait] callback
//! is implemented internally via `ReadDirectoryChangesW` and fires
//! for every change under the sync root — including newly-created
//! files and directories the user dropped in from outside. We use
//! that signal to push new content to the server without adding a
//! second filesystem watcher.
//!
//! ## What gets uploaded
//!
//! M0.5 scope: **new files and directories only**. The check is
//! `HEAD /fs/{path}` returning 404 — if the server already knows
//! about a path, we leave it alone. Edits to existing files are a
//! follow-up that requires placeholder conversion + dirty-bit
//! tracking.
//!
//! ## Loop guards
//!
//! Two ways for our own writes to come back through state_changed:
//!
//! 1. **Hydrated placeholders.** When [`SyncFilter::fetch_data`]
//!    populates a placeholder, the local file is briefly written.
//!    Cloud Filter placeholders are NTFS reparse points
//!    ([FILE_ATTRIBUTE_REPARSE_POINT]) — we filter them out so we
//!    never re-upload content we just hydrated.
//! 2. **Server already knows.** Even for non-placeholder files, a
//!    second state_changed event for the same path (e.g. metadata
//!    change) would otherwise trigger a re-upload. The HEAD probe
//!    short-circuits.
//!
//! [trait]: cloud_filter::filter::SyncFilter::state_changed
//! [FILE_ATTRIBUTE_REPARSE_POINT]: https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use garden_common::client::StoneApi;
use tokio::runtime::Handle;
use tracing::{debug, info, warn};

/// Brief debounce before processing a state_changed entry. Coalesces
/// the spurious duplicate events `ReadDirectoryChangesW` produces
/// during file create-and-write sequences (Explorer first creates
/// the empty file, then writes the bytes — two events, ms apart).
const PROCESS_DELAY: Duration = Duration::from_millis(500);

pub struct Uploader {
    api: Arc<StoneApi>,
    sync_root: PathBuf,
    rt: Handle,
}

impl Uploader {
    pub fn new(api: Arc<StoneApi>, sync_root: PathBuf, rt: Handle) -> Self {
        Self {
            api,
            sync_root,
            rt,
        }
    }

    /// Process a batch of changed paths from `state_changed`. Spawns
    /// one task per path so the callback (running on a Cloud Filter
    /// threadpool thread) returns immediately.
    pub fn handle_changes(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        debug!(count = paths.len(), "uploader: processing state_changed batch");
        for path in paths {
            let api = self.api.clone();
            let sync_root = self.sync_root.clone();
            self.rt.spawn(async move {
                if let Err(e) = process_one(&api, &sync_root, &path).await {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "uploader: processing failed"
                    );
                }
            });
        }
    }
}

async fn process_one(api: &StoneApi, sync_root: &Path, path: &Path) -> anyhow::Result<()> {
    // Brief debounce — Explorer's create-then-write sequence fires
    // back-to-back events; sleeping briefly lets the file settle
    // before we read it.
    tokio::time::sleep(PROCESS_DELAY).await;

    // Skip if path no longer exists — state_changed fires for
    // deletes too, and the user might have moved/removed before
    // we got around to processing.
    let metadata = match tokio::fs::metadata(path).await {
        Ok(m) => m,
        Err(_) => {
            debug!(path = %path.display(), "uploader: path no longer exists, skipping");
            return Ok(());
        }
    };

    // Loop guard 1: placeholders are content we hydrated, not new
    // local content. Re-uploading them would create an infinite
    // round-trip on every fetch_data.
    if is_placeholder(&metadata) {
        debug!(path = %path.display(), "uploader: placeholder, skipping");
        return Ok(());
    }

    let (storage, rel) = match resolve_path(sync_root, path) {
        Some(r) => r,
        None => {
            debug!(path = %path.display(), "uploader: not under sync root, skipping");
            return Ok(());
        }
    };

    // The sync root and storage root themselves aren't actionable
    // — Pavilion doesn't create storages, only contents within them.
    if storage.is_empty() || rel.is_empty() {
        return Ok(());
    }

    // Loop guard 2: skip paths the server already knows about.
    // M0.5 scope is new content only; edit-and-push requires
    // placeholder conversion + dirty tracking which is a follow-up.
    if api.garden().storage().path_exists(&storage, &rel).await? {
        debug!(
            storage = %storage,
            path = %rel,
            "uploader: server already knows this path, skipping"
        );
        return Ok(());
    }

    if metadata.is_dir() {
        info!(
            storage = %storage,
            path = %rel,
            "uploader: creating directory on server"
        );
        api.garden()
            .storage()
            .create_directory(&storage, &rel)
            .await?;
    } else {
        let bytes = tokio::fs::read(path).await?;
        info!(
            storage = %storage,
            path = %rel,
            size = bytes.len(),
            "uploader: pushing new file to server"
        );
        api.garden()
            .storage()
            .write_file(&storage, &rel, bytes)
            .await?;
    }

    Ok(())
}

/// Decompose a Cloud Filter request path into `(storage, relative)`.
/// Mirrors [`super::provider::PavilionProvider::resolve_path`] but
/// is duplicated rather than shared because the upload side may
/// evolve different filtering (e.g., dotfile suppression) than the
/// hydration side.
fn resolve_path(sync_root: &Path, target: &Path) -> Option<(String, String)> {
    let rel = target.strip_prefix(sync_root).ok()?;
    let mut components = rel.components();
    let storage_name = match components.next() {
        Some(c) => c.as_os_str().to_string_lossy().to_string(),
        None => return Some((String::new(), String::new())),
    };
    let remainder: PathBuf = components.collect();
    Some((storage_name, remainder.to_string_lossy().replace('\\', "/")))
}

#[cfg(target_os = "windows")]
fn is_placeholder(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // Cloud Filter placeholders are reparse points. Plain files
    // and directories the user creates have this bit clear.
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_placeholder(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(r"C:\Users\test\Zen Garden")
    }

    #[test]
    fn resolve_under_storage_returns_storage_and_rel() {
        let r = resolve_path(&root(), &root().join("storage").join("foo.txt"))
            .expect("must resolve");
        assert_eq!(r.0, "storage");
        assert_eq!(r.1, "foo.txt");
    }

    #[test]
    fn resolve_nested_normalises_to_forward_slashes() {
        let r = resolve_path(
            &root(),
            &root().join("storage").join("Tax Documents").join("a.pdf"),
        )
        .expect("must resolve");
        assert_eq!(r.0, "storage");
        assert_eq!(r.1, "Tax Documents/a.pdf");
        assert!(!r.1.contains('\\'));
    }

    #[test]
    fn resolve_outside_root_returns_none() {
        let r = resolve_path(&root(), &PathBuf::from(r"C:\Other\foo.txt"));
        assert!(r.is_none());
    }

    #[test]
    fn resolve_sync_root_itself_returns_empty_pair() {
        let r = resolve_path(&root(), &root()).expect("sync root must resolve");
        assert_eq!(r.0, "");
        assert_eq!(r.1, "");
    }
}
