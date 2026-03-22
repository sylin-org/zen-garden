//! Sync root ingest — write-back path for Cloud Filter (STORAGE-0012)
//!
//! When a user pastes, drags, or saves a file into the Explorer "Zen Garden"
//! sync root, this module detects the new file, copies it to the actual
//! storage mount (or proxies to the remote Primary), and marks it as in-sync
//! (clearing the overlay icon).
//!
//! ## Pipeline
//!
//! 1. **Monitor** — `notify` crate watches the sync root for Create/Modify
//! 2. **Filter** — skip placeholders, metadata dirs, top-level storage folders
//! 3. **Transfer** — copy file from sync root to storage via `StorageHandle`
//! 4. **Sync state** — `CfConvertToPlaceholder` marks the file in-sync
//!
//! The storage mount watcher (`infra/storage/watcher.rs`) independently detects
//! the copied file and records a changelog entry — so replication picks it up
//! without any coupling here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use notify::Watcher;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use tokio::sync::broadcast;

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use crate::infra::storage::handle::StorageResolver;
use garden_common::storage::{StorageChanged, StorageTick};

// ============================================================================
// Constants
// ============================================================================

/// Debounce window for ingest events (matches storage watcher).
const DEBOUNCE_SECS: u64 = 2;

// ============================================================================
// Public API
// ============================================================================

/// Run the sync root ingest watcher until shutdown.
///
/// Monitors the sync root for user-created files and copies them to the
/// corresponding storage mount (or proxies to remote Primary via
/// `StorageHandle` with tick notifications).  Runs as a long-lived task.
///
/// Listens on `storage_changed_rx` so that files pasted while a storage was
/// offline are retried as soon as the storage comes back online.
pub(crate) async fn run(
    volumes: Volumes,
    registry: GardenRegistry,
    stone_id: String,
    tick: broadcast::Sender<StorageTick>,
    sync_root_path: PathBuf,
    mut storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    shutdown_token: CancellationToken,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Event>(256);

    let _watcher = match spawn_watcher(&sync_root_path, tx) {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "failed to start sync root ingest watcher");
            return;
        }
    };

    info!("sync root ingest watcher started");

    // Catch files pasted before the watcher started
    initial_scan(&volumes, &registry, &stone_id, &tick, &sync_root_path).await;

    let debounce = tokio::time::Duration::from_secs(DEBOUNCE_SECS);
    let mut pending: HashMap<PathBuf, ()> = HashMap::new();
    let mut debounce_deadline = tokio::time::Instant::now() + debounce;

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                debug!("sync root ingest watcher shutting down");
                break;
            }
            event = rx.recv() => {
                let Some(event) = event else { break; };

                match event.kind {
                    notify::EventKind::Create(_) | notify::EventKind::Modify(_) => {}
                    _ => continue,
                }

                for path in &event.paths {
                    if should_ingest(path, &sync_root_path) {
                        pending.insert(path.clone(), ());
                    }
                }

                debounce_deadline = tokio::time::Instant::now() + debounce;
            }
            result = storage_changed_rx.recv() => {
                let should_rescan = match result {
                    Ok(StorageChanged::Added { .. }) | Ok(StorageChanged::Reclassified) => true,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => false,
                };
                if should_rescan {
                    debug!("storage came online — re-scanning sync root for pending files");
                    initial_scan(&volumes, &registry, &stone_id, &tick, &sync_root_path).await;
                }
            }
            _ = tokio::time::sleep_until(debounce_deadline) => {
                if !pending.is_empty() {
                    let batch: Vec<PathBuf> = pending.drain().map(|(p, _)| p).collect();
                    transfer_batch(&volumes, &registry, &stone_id, &tick, &sync_root_path, &batch).await;
                }

                debounce_deadline = tokio::time::Instant::now() + debounce;
            }
        }
    }
}

// ============================================================================
// Event filtering
// ============================================================================

/// Whether a path should be ingested from the sync root.
///
/// Rejects:
/// - Top-level directories (storage placeholders — managed separately)
/// - `.zen-garden` metadata at any depth
/// - Dehydrated CfApi placeholders (would cause hydration loops)
fn should_ingest(path: &Path, sync_root_path: &Path) -> bool {
    let (storage_name, remainder) = match super::decompose_sync_root_path(path, sync_root_path) {
        Some(r) => r,
        None => return false,
    };

    // Need storage_name + at least one path component
    if storage_name.is_empty() || remainder.as_os_str().is_empty() {
        return false;
    }

    // Skip metadata directories at any depth
    let rel = match path.strip_prefix(sync_root_path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for c in rel.components() {
        if let std::path::Component::Normal(s) = c {
            let s = s.to_string_lossy();
            if garden_common::constants::storage::share::is_blocked_name(&s) {
                return false;
            }
        }
    }

    // Skip dehydrated placeholders — reading them triggers fetch_data
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        if let Ok(meta) = std::fs::metadata(path)
            && meta.file_attributes() & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS != 0 {
                return false;
            }
    }

    true
}

// ============================================================================
// File transfer
// ============================================================================

/// Copy a batch of files from the sync root to their storage via `StorageHandle`.
async fn transfer_batch(
    volumes: &Volumes,
    registry: &GardenRegistry,
    stone_id: &str,
    tick: &broadcast::Sender<StorageTick>,
    sync_root_path: &Path,
    paths: &[PathBuf],
) {
    let mut ingested = 0u32;

    for path in paths {
        let (storage_name, remainder) = match super::decompose_sync_root_path(path, sync_root_path)
        {
            Some((s, r)) if !s.is_empty() && !r.as_os_str().is_empty() => (s, r),
            _ => continue,
        };

        let resolver = StorageResolver {
            volumes,
            registry,
            stone_id,
            tick: Some(tick.clone()),
        };
        let handle = match resolver.for_write(&storage_name).await {
            Ok(r) => r,
            Err(_) => {
                debug!(storage = %storage_name, "no writable route for ingest");
                continue;
            }
        };

        let rel_path = remainder.to_string_lossy().replace('\\', "/");

        // For local storages, check if content is already identical
        if handle.is_local()
            && let Some(mp) = handle.mount_path() {
                let target = mp.join(&remainder);
                if target.exists() && files_match(path, &target).await {
                    mark_in_sync(path);
                    continue;
                }
            }

        // Transfer via handle (handles both local and remote)
        if path.is_dir() {
            match handle.mkdir(&rel_path).await {
                Ok(()) => {
                    debug!(
                        storage = %storage_name,
                        path = %rel_path,
                        "ingest: directory created"
                    );
                    mark_in_sync(path);
                    ingested += 1;
                }
                Err(e) => {
                    warn!(
                        storage = %storage_name,
                        path = %rel_path,
                        error = %e,
                        "ingest: mkdir failed"
                    );
                }
            }
        } else if path.is_file() {
            match tokio::fs::read(path).await {
                Ok(data) => match handle.write(&rel_path, &data).await {
                    Ok(()) => {
                        debug!(
                            storage = %storage_name,
                            path = %rel_path,
                            bytes = data.len(),
                            "ingest: file written"
                        );
                        mark_in_sync(path);
                        ingested += 1;
                    }
                    Err(e) => {
                        warn!(
                            storage = %storage_name,
                            path = %rel_path,
                            error = %e,
                            "ingest: write failed"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        storage = %storage_name,
                        path = %rel_path,
                        error = %e,
                        "ingest: read source failed"
                    );
                }
            }
        }
    }

    if ingested > 0 {
        info!(count = ingested, "ingested files from Explorer to storage");
    }
}

/// Initial scan: catch files created before the watcher started.
async fn initial_scan(
    volumes: &Volumes,
    registry: &GardenRegistry,
    stone_id: &str,
    tick: &broadcast::Sender<StorageTick>,
    sync_root_path: &Path,
) {
    let mut pending = Vec::new();
    let mut stack = vec![sync_root_path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(d) => d,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if garden_common::constants::storage::share::is_blocked_name(&name_str) {
                continue;
            }

            let is_dir = entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);

            if should_ingest(&path, sync_root_path) {
                pending.push(path.clone());
            }

            if is_dir {
                stack.push(path);
            }
        }
    }

    if !pending.is_empty() {
        debug!(count = pending.len(), "initial ingest scan found files");
        transfer_batch(volumes, registry, stone_id, tick, sync_root_path, &pending).await;
    }
}

// ============================================================================
// Content comparison
// ============================================================================

/// Streaming byte-level comparison of two files.
///
/// Returns `true` if both files exist, have the same size, and identical content.
/// Returns `false` on any I/O error or mismatch (safe default: don't skip).
/// Directories always return `false` (let mkdir handle idempotency).
async fn files_match(a: &Path, b: &Path) -> bool {
    use tokio::io::AsyncReadExt;

    // Directories can't be compared this way
    if a.is_dir() || b.is_dir() {
        return false;
    }

    let (mut fa, mut fb) = match (
        tokio::fs::File::open(a).await,
        tokio::fs::File::open(b).await,
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return false,
    };

    // Quick size check
    let (ma, mb) = match (fa.metadata().await, fb.metadata().await) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return false,
    };
    if ma.len() != mb.len() {
        return false;
    }

    // Stream compare in 64 KiB chunks
    let mut buf_a = vec![0u8; 64 * 1024];
    let mut buf_b = vec![0u8; 64 * 1024];
    loop {
        let n_a = fa.read(&mut buf_a).await.unwrap_or(0);
        let n_b = fb.read(&mut buf_b).await.unwrap_or(0);
        if n_a != n_b || buf_a[..n_a] != buf_b[..n_b] {
            return false;
        }
        if n_a == 0 {
            return true;
        }
    }
}

// ============================================================================
// CfApi sync state
// ============================================================================

/// Mark a file/directory in the sync root as in-sync, clearing the overlay.
///
/// Converts to a CfApi placeholder with `CF_CONVERT_FLAG_MARK_IN_SYNC`.
fn mark_in_sync(path: &Path) {
    #[cfg(windows)]
    {
        use cloud_filter::placeholder::{ConvertOptions, Placeholder};
        use std::os::windows::fs::OpenOptionsExt;

        // Win32 access rights for directory handles (open as directory placeholder).
        const DIR_ACCESS: u32 = 0x0012_0003; // FILE_LIST_DIRECTORY | FILE_ADD_FILE | SYNCHRONIZE | READ_CONTROL
        const DIR_SHARE_ALL: u32 = 7; // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        const BACKUP_SEMANTICS: u32 = 0x0200_0000; // FILE_FLAG_BACKUP_SEMANTICS

        let open_result = if path.is_dir() {
            std::fs::OpenOptions::new()
                .access_mode(DIR_ACCESS)
                .share_mode(DIR_SHARE_ALL)
                .custom_flags(BACKUP_SEMANTICS)
                .open(path)
        } else {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
        };

        let file = match open_result {
            Ok(f) => f,
            Err(e) => {
                debug!(path = %path.display(), error = %e, "mark_in_sync: open failed");
                return;
            }
        };

        let mut ph = Placeholder::from(file);
        match ph.convert_to_placeholder(ConvertOptions::default().mark_in_sync().force(), None) {
            Ok(_) => debug!(path = %path.display(), "marked in-sync"),
            Err(e) => debug!(path = %path.display(), error = %e, "convert_to_placeholder failed"),
        }
    }
}

// ============================================================================
// Filesystem watcher
// ============================================================================

/// Spawn a `notify` watcher feeding events to a tokio channel.
fn spawn_watcher(
    path: &Path,
    tx: tokio::sync::mpsc::Sender<notify::Event>,
) -> Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                let _ = tx.try_send(event);
            }
            Err(e) => {
                warn!(error = %e, "sync root watcher error");
            }
        },
    )
    .context("failed to create sync root watcher")?;

    watcher
        .watch(path, notify::RecursiveMode::Recursive)
        .context("failed to watch sync root")?;

    Ok(watcher)
}
