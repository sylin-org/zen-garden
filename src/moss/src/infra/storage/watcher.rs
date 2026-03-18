//! Filesystem watcher — detect external writes for replication (STORAGE-0009 Phase 5)
//!
//! When users write files directly to a managed storage mount (e.g. via
//! Explorer, `cp`, or another app), those writes bypass Moss's API and
//! don't appear in the changelog. This module watches the storage root
//! for filesystem events and records changelog entries so replication
//! stays coherent.
//!
//! ## Design
//!
//! - Uses the `notify` crate (cross-platform: inotify on Linux, ReadDirectoryChanges on Windows, kqueue on macOS)
//! - One watcher per managed storage, watching the mount root recursively
//! - Events inside `.zen-garden/` are ignored (managed by Moss itself)
//! - Events are debounced (2s window) to batch rapid writes
//! - Each event produces a changelog entry via `ContentStore::record_external_change`
//! - The watcher is spawned per-storage and cancelled on unmount/shutdown
//!
//! ## Architecture
//!
//! Thin infra layer — watches the filesystem and emits changelog entries.
//! No business logic. The domain layer (replication, StorageService) consumes
//! the changelog as usual — it doesn't know whether entries came from API
//! writes or filesystem events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use garden_common::constants::paths;
use garden_common::storage::{ChangelogEntry, ChangelogOp, StorageTick};
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::Volumes;

// ============================================================================
// Constants
// ============================================================================

/// Debounce window for filesystem events.
///
/// Batches rapid writes (e.g. `cp -r` dumping many files) into fewer
/// changelog entries. Matches the storage beacon cadence (10s) divided
/// by 5 for reasonable responsiveness.
const DEBOUNCE_SECS: u64 = 2;

// ============================================================================
// Public API
// ============================================================================

/// State for the filesystem watcher system — one watcher per storage.
pub struct StorageWatcherSet {
    /// Active watchers keyed by storage ID.
    watchers: Arc<RwLock<HashMap<String, ActiveWatcher>>>,
    /// Shared volumes for looking up managed storage mount paths.
    volumes: Volumes,
    /// Storage tick sender for replication notifications.
    tick_tx: broadcast::Sender<StorageTick>,
    /// Parent shutdown token.
    shutdown_token: CancellationToken,
}

/// Handle to a running watcher for a single storage.
struct ActiveWatcher {
    /// Cancels this specific watcher.
    cancel: CancellationToken,
    /// Storage name (for logging).
    #[allow(dead_code)]
    name: String,
}

impl StorageWatcherSet {
    /// Create a new watcher set.
    pub fn new(
        volumes: Volumes,
        tick_tx: broadcast::Sender<StorageTick>,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            watchers: Arc::new(RwLock::new(HashMap::new())),
            volumes,
            tick_tx,
            shutdown_token,
        }
    }

    /// Reconcile watchers with the current set of managed storages.
    ///
    /// Call periodically (e.g. every 30s from the coordinator). Starts
    /// watchers for new storages, stops watchers for departed ones.
    pub async fn reconcile(&self) {
        let current_storages: Vec<(String, String, String, PathBuf)> = {
            let map = self.volumes.read().await;
            map.values()
                .filter_map(|v| {
                    let mgmt = v.management.as_ref()?;
                    Some((
                        mgmt.id.clone(),
                        mgmt.name.clone(),
                        mgmt.replica_set_id.clone(),
                        v.mount_path.clone(),
                    ))
                })
                .collect()
        };

        let mut watchers = self.watchers.write().await;

        // Start watchers for new storages
        for (id, name, replica_set_id, mount_path) in &current_storages {
            if watchers.contains_key(id) {
                continue;
            }

            let cancel = self.shutdown_token.child_token();
            let tick_tx = self.tick_tx.clone();
            let storage_name = name.clone();
            let rs_id = replica_set_id.clone();
            let watch_path = mount_path.clone();
            let watcher_cancel = cancel.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    run_storage_watcher(&watch_path, &storage_name, &rs_id, tick_tx, watcher_cancel)
                        .await
                {
                    warn!(
                        storage = %storage_name,
                        error = %e,
                        "Filesystem watcher exited with error"
                    );
                }
            });

            info!(storage = %name, path = %mount_path.display(), "Filesystem watcher started");
            watchers.insert(
                id.clone(),
                ActiveWatcher {
                    cancel,
                    name: name.clone(),
                },
            );
        }

        // Stop watchers for departed storages
        let current_ids: std::collections::HashSet<&String> =
            current_storages.iter().map(|(id, _, _, _)| id).collect();

        let departed: Vec<String> = watchers
            .keys()
            .filter(|id| !current_ids.contains(id))
            .cloned()
            .collect();

        for id in departed {
            if let Some(w) = watchers.remove(&id) {
                w.cancel.cancel();
                info!(storage = %w.name, "Filesystem watcher stopped (storage departed)");
            }
        }
    }
}

// ============================================================================
// Per-storage watcher task
// ============================================================================

/// Run a filesystem watcher for a single managed storage.
///
/// Watches the mount root recursively, ignoring `.zen-garden/` events.
/// Debounces events and records changelog entries.
async fn run_storage_watcher(
    mount_path: &Path,
    storage_name: &str,
    replica_set_id: &str,
    tick_tx: broadcast::Sender<StorageTick>,
    cancel: CancellationToken,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Event>(256);

    // Create the notify watcher on a blocking thread (it uses OS APIs)
    let watch_path = mount_path.to_path_buf();
    let _watcher = spawn_notify_watcher(watch_path, tx)?;

    let dotfolder = paths::STORAGE_DOTFOLDER;
    let debounce = Duration::from_secs(DEBOUNCE_SECS);
    let mut pending: HashMap<PathBuf, ChangelogOp> = HashMap::new();
    let mut debounce_deadline = tokio::time::Instant::now() + debounce;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(storage = %storage_name, "Filesystem watcher cancelled");
                break;
            }
            event = rx.recv() => {
                let Some(event) = event else {
                    // Watcher dropped — channel closed
                    debug!(storage = %storage_name, "Filesystem watcher channel closed");
                    break;
                };

                // Filter: ignore .zen-garden/ and "Zen Garden" symlink events
                let dominated_by_managed = event.paths.iter().all(|p| {
                    let rel = p.strip_prefix(mount_path).unwrap_or(p);
                    let first = rel.components().next();
                    matches!(first, Some(std::path::Component::Normal(s)) if {
                        let s = s.to_string_lossy();
                        s == dotfolder || s == "Zen Garden"
                    })
                });

                if dominated_by_managed {
                    continue;
                }

                // Map event kind to changelog op
                let op = match event.kind {
                    EventKind::Create(_) => ChangelogOp::C,
                    EventKind::Modify(_) => ChangelogOp::M,
                    EventKind::Remove(_) => ChangelogOp::D,
                    _ => continue,
                };

                // Accumulate into pending map (last-writer-wins per path)
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(mount_path) {
                        pending.insert(rel.to_path_buf(), op);
                    }
                }

                // Reset debounce deadline
                debounce_deadline = tokio::time::Instant::now() + debounce;
            }
            _ = tokio::time::sleep_until(debounce_deadline) => {
                if pending.is_empty() {
                    // Nothing to flush — sleep longer
                    debounce_deadline = tokio::time::Instant::now() + debounce;
                    continue;
                }

                // Flush pending events as changelog entries
                let batch: Vec<(PathBuf, ChangelogOp)> = pending.drain().collect();
                flush_changelog_batch(
                    mount_path,
                    storage_name,
                    replica_set_id,
                    &batch,
                    &tick_tx,
                ).await;

                debounce_deadline = tokio::time::Instant::now() + debounce;
            }
        }
    }

    Ok(())
}

/// Spawn the `notify` watcher on the current runtime.
///
/// Returns the watcher handle (must stay alive to keep receiving events).
fn spawn_notify_watcher(
    path: PathBuf,
    tx: tokio::sync::mpsc::Sender<notify::Event>,
) -> Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // Non-blocking send — if the channel is full, drop the event
                    // (the debounce window will catch the next one)
                    let _ = tx.try_send(event);
                }
                Err(e) => {
                    warn!(error = %e, "Filesystem watcher error");
                }
            }
        },
    )
    .context("Failed to create filesystem watcher")?;

    watcher
        .watch(&path, RecursiveMode::Recursive)
        .context("Failed to start watching path")?;

    Ok(watcher)
}

// ============================================================================
// Changelog integration
// ============================================================================

/// Flush a batch of filesystem events as changelog entries.
///
/// Records each changed path in the changelog (`.zen-garden/changelog.jsonl`)
/// and emits a summary `StorageTick`.
async fn flush_changelog_batch(
    mount_path: &Path,
    storage_name: &str,
    replica_set_id: &str,
    batch: &[(PathBuf, ChangelogOp)],
    tick_tx: &broadcast::Sender<StorageTick>,
) {
    let changelog_path = mount_path.join(".zen-garden/changelog.jsonl");

    // Ensure directory exists
    if let Some(parent) = changelog_path.parent() {
        if !parent.exists() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
    }

    let mut creates = 0u32;
    let mut modifies = 0u32;
    let mut deletes = 0u32;
    let mut last_cursor = String::new();

    // Build all changelog lines in memory, then write once
    let mut lines = String::new();
    for (rel_path, op) in batch {
        let path_str = rel_path.to_string_lossy().replace('\\', "/");
        let entry = match op {
            ChangelogOp::C => ChangelogEntry::created(&path_str, 0),
            ChangelogOp::M => ChangelogEntry::modified(&path_str, 0),
            ChangelogOp::D => ChangelogEntry::deleted(&path_str),
        };

        match op {
            ChangelogOp::C => creates += 1,
            ChangelogOp::M => modifies += 1,
            ChangelogOp::D => deletes += 1,
        }

        last_cursor = entry.c.clone();

        if let Ok(json) = serde_json::to_string(&entry) {
            lines.push_str(&json);
            lines.push('\n');
        }
    }

    // Append to changelog
    if !lines.is_empty() {
        use tokio::io::AsyncWriteExt;
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&changelog_path)
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(lines.as_bytes()).await {
                    warn!(
                        storage = %storage_name,
                        error = %e,
                        "Failed to append fs-watcher changelog entries"
                    );
                }
            }
            Err(e) => {
                warn!(
                    storage = %storage_name,
                    error = %e,
                    "Failed to open changelog for fs-watcher entries"
                );
            }
        }
    }

    // Emit aggregated tick
    if !last_cursor.is_empty() {
        let tick = StorageTick {
            cursor: last_cursor,
            storage: storage_name.to_string(),
            replica_set_id: replica_set_id.to_string(),
            creates,
            modifies,
            deletes,
        };
        let _ = tick_tx.send(tick);

        debug!(
            storage = %storage_name,
            creates,
            modifies,
            deletes,
            "Filesystem watcher: flushed external changes to changelog"
        );
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dotfolder_filtering() {
        // Verify our filtering logic catches .zen-garden paths
        let mount = Path::new("/mnt/storage");
        let zen_path = mount.join(paths::STORAGE_DOTFOLDER).join("manifest.json");
        let rel = zen_path.strip_prefix(mount).unwrap();
        let first = rel.components().next();
        assert!(
            matches!(first, Some(std::path::Component::Normal(s)) if s.to_string_lossy() == paths::STORAGE_DOTFOLDER)
        );

        // User files should NOT be filtered
        let user_path = mount.join("Photos/vacation.jpg");
        let rel = user_path.strip_prefix(mount).unwrap();
        let first = rel.components().next();
        assert!(
            matches!(first, Some(std::path::Component::Normal(s)) if s.to_string_lossy() == "Photos")
        );
    }
}
