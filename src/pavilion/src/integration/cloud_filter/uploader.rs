//! Filesystem-watcher-driven upload (PAVILION-0002 §"Add Cloud
//! Filter upload to M1 critical path").
//!
//! ## Why our own watcher
//!
//! Cloud Filter's [`SyncFilter::state_changed`][trait] looks
//! tempting at first glance — it's already wired and it does
//! receive paths via `ReadDirectoryChangesW`. But the cloud-filter
//! crate calls `ReadDirectoryChangesW` with
//! `FILE_NOTIFY_CHANGE_ATTRIBUTES` as the filter mask, which
//! reports pin/unpin / dehydrate transitions only — never
//! file or directory creation. We need a wider mask, so we run
//! our own [`notify`] watcher in parallel.
//!
//! ## What gets uploaded
//!
//! New files, edited files, and new directories under the sync
//! root. Discrimination uses an in-memory mtime cache:
//!
//! - **Cache miss + server miss** → new content; upload, then
//!   record `mtime` in the cache.
//! - **Cache miss + server hit** → freshly-hydrated placeholder
//!   we're seeing for the first time; record `mtime` silently and
//!   skip.
//! - **Cache hit, local `mtime` advanced** → user edit; upload,
//!   update cache.
//! - **Cache hit, local `mtime` unchanged** → no-op event; skip.
//!
//! ## Loop guards
//!
//! Three ways our own writes could echo back through the watcher:
//!
//! 1. **Hydrated placeholders.** When [`SyncFilter::fetch_data`]
//!    populates a placeholder, the local file is briefly written.
//!    Cloud Filter placeholders are NTFS reparse points
//!    ([FILE_ATTRIBUTE_REPARSE_POINT]) — we filter them out so we
//!    never re-upload content we just hydrated.
//! 2. **Same-mtime echo.** Filesystem APIs sometimes fire a Modify
//!    event without an mtime change (touched timestamp, attribute
//!    flip). The cache equality check skips those.
//! 3. **First sighting of an existing path.** If the user opens a
//!    file we haven't tracked yet, the cache is empty but the
//!    server already has the content. The cache-miss-with-
//!    server-hit branch records the current mtime silently rather
//!    than re-uploading the bytes back to where they came from.
//!
//! [trait]: cloud_filter::filter::SyncFilter::state_changed
//! [FILE_ATTRIBUTE_REPARSE_POINT]: https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use garden_common::client::StoneApi;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, warn};

/// Per-path "last successfully uploaded `mtime`" cache. Drives the
/// edit-vs-echo discrimination so an event for an already-pushed
/// version of a file doesn't re-upload it. Keys are
/// `"{storage}:{rel_path}"` strings.
type UploadedCache = Arc<Mutex<HashMap<String, SystemTime>>>;

/// Brief debounce before processing a watcher event. Coalesces the
/// duplicate events filesystem APIs produce during create-and-write
/// sequences (Explorer first creates the empty file, then writes
/// the bytes — two events, ms apart).
const PROCESS_DELAY: Duration = Duration::from_millis(500);

/// Channel buffer for the watcher → processor handoff. Modest
/// capacity keeps memory bounded; the watcher's own try_send drops
/// events when full (the debounce / HEAD-probe pipeline catches up
/// on the next event for the same path).
const WATCHER_CHANNEL_CAPACITY: usize = 256;

/// Owns the filesystem watcher and a handle the provider can drop
/// to stop it. Constructed once per Cloud Filter session and held
/// alive for the process lifetime — when [`Uploader`] drops, the
/// watcher's worker thread exits.
pub struct Uploader {
    // Holding the watcher here keeps it alive. notify's
    // RecommendedWatcher stops watching when dropped.
    _watcher: notify::RecommendedWatcher,
}

impl Uploader {
    /// Build the watcher and spawn the processor task. Returns
    /// `Err` only when the OS-level watcher refuses to start —
    /// the worker task itself can't fail in a way that surfaces
    /// here.
    pub fn start(api: Arc<StoneApi>, sync_root: PathBuf, rt: Handle) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<notify::Event>(WATCHER_CHANNEL_CAPACITY);

        let mut watcher = notify::recommended_watcher(
            move |res: Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    let _ = tx.try_send(event);
                }
                Err(e) => {
                    warn!(error = %e, "uploader: watcher error");
                }
            },
        )
        .context("uploader: failed to create filesystem watcher")?;

        watcher
            .watch(&sync_root, RecursiveMode::Recursive)
            .with_context(|| {
                format!(
                    "uploader: failed to watch sync root {}",
                    sync_root.display()
                )
            })?;

        info!(sync_root = %sync_root.display(), "uploader: watcher started");

        // Processor task — consumes the watcher channel and drives
        // each candidate path through `process_one`.
        let uploaded: UploadedCache = Arc::new(Mutex::new(HashMap::new()));
        rt.spawn(run_processor(api, sync_root, rx, uploaded));

        Ok(Self { _watcher: watcher })
    }
}

async fn run_processor(
    api: Arc<StoneApi>,
    sync_root: PathBuf,
    mut rx: mpsc::Receiver<notify::Event>,
    uploaded: UploadedCache,
) {
    while let Some(event) = rx.recv().await {
        // Create and Modify both push to the server. Remove fires
        // when the user deletes via Explorer; that path is handled
        // by Cloud Filter's delete callback (provider.rs) and we
        // skip it here to avoid double-handling.
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_)
        ) {
            continue;
        }

        for path in event.paths {
            let api = api.clone();
            let sync_root = sync_root.clone();
            let uploaded = uploaded.clone();
            tokio::spawn(async move {
                if let Err(e) = process_one(&api, &sync_root, &path, &uploaded).await {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "uploader: processing failed"
                    );
                }
            });
        }
    }
    debug!("uploader: processor channel closed, exiting");
}

async fn process_one(
    api: &StoneApi,
    sync_root: &Path,
    path: &Path,
    uploaded: &UploadedCache,
) -> anyhow::Result<()> {
    // Brief debounce — Explorer's create-then-write sequence fires
    // back-to-back events; sleeping briefly lets the file settle
    // before we read it.
    tokio::time::sleep(PROCESS_DELAY).await;

    // Skip if path no longer exists — `notify` events are reported
    // out of order in some cases, and the user might have moved /
    // removed before we got around to processing.
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

    let local_mtime = metadata.modified().ok();
    let key = format!("{storage}:{rel}");

    // Loop guard 2: same-mtime echo. If we've already uploaded
    // this exact mtime, nothing to do.
    let cached = { uploaded.lock().await.get(&key).copied() };
    if let (Some(local), Some(cached)) = (local_mtime, cached)
        && local <= cached
    {
        debug!(
            storage = %storage,
            path = %rel,
            "uploader: mtime unchanged since last upload, skipping"
        );
        return Ok(());
    }

    // Cache miss: ask the server whether it already knows this
    // path. If yes, this is a freshly-hydrated placeholder we're
    // seeing for the first time — record its mtime silently and
    // skip the upload (we'd just be sending the bytes back to
    // where they came from). If no, it's new local content.
    let server_exists = api.garden().storage().path_exists(&storage, &rel).await?;
    if cached.is_none() && server_exists {
        debug!(
            storage = %storage,
            path = %rel,
            "uploader: first sighting of existing path, recording mtime silently"
        );
        if let Some(local) = local_mtime {
            uploaded.lock().await.insert(key, local);
        }
        return Ok(());
    }

    // Either: cache miss + server miss (new content) or cache hit
    // + local mtime advanced (edit). Both paths upload.
    if metadata.is_dir() {
        // Skip dir Modify events — directory mtime advances when
        // children change, but the directory itself has nothing
        // new to push. Children fire their own events.
        if cached.is_some() {
            return Ok(());
        }
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
        let log_kind = if cached.is_some() { "edit" } else { "new" };
        info!(
            storage = %storage,
            path = %rel,
            size = bytes.len(),
            kind = log_kind,
            "uploader: pushing file to server"
        );
        api.garden()
            .storage()
            .write_file(&storage, &rel, bytes)
            .await?;
    }

    if let Some(local) = local_mtime {
        uploaded.lock().await.insert(key, local);
    }

    Ok(())
}

/// Decompose a path into `(storage, relative)` against the sync
/// root. Mirrors `provider::resolve_path` but is duplicated so the
/// upload side can evolve different filtering than the hydration
/// side without coupling them.
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
