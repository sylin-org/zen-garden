//! Cloud Filter integration (STORAGE-0009 Phase 4, rebuilt per STORAGE-0012)
//!
//! Registers a Windows Cloud Sync Provider so managed storages appear
//! natively in Explorer under "Zen Garden".  Files are fetched on demand
//! from the hosting stone's storage API.
//!
//! ## Module layout
//!
//! - `registration.rs` — sync root registration lifecycle
//! - `provider.rs`     — CfApi `Filter` trait callbacks (download path)
//! - `ingest.rs`       — write-back from Explorer (upload path)
//! - `placeholders.rs` — shared placeholder helpers (valid timestamps)
//!
//! ## Lifecycle
//!
//! 1. `start()` ensures the sync root is registered (idempotent)
//! 2. Connects the `ZenGardenProvider` to the sync root
//! 3. Spawns a storage watcher that creates/removes placeholder dirs
//! 4. Spawns an ingest watcher that copies user-pasted files to storage
//! 5. On shutdown, the connection drops (disconnects the provider)

mod ingest;
mod placeholders;
mod provider;
mod registration;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use cloud_filter::root::Session;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use garden_common::storage::{StorageChanged, StorageTick, DEFAULT_REPLICA_SET_DISPLAY};

use self::provider::ZenGardenProvider;

// ============================================================================
// Shared utilities
// ============================================================================

/// Decompose a path under the sync root into (storage_name, relative_path).
///
/// Returns `None` if the path is not under the sync root.
/// Returns `("", PathBuf::new())` if the path IS the sync root.
pub(crate) fn decompose_sync_root_path(
    path: &Path,
    sync_root_path: &Path,
) -> Option<(String, PathBuf)> {
    let rel = path.strip_prefix(sync_root_path).ok()?;
    let mut components = rel.components();
    let storage_name = match components.next() {
        Some(c) => c.as_os_str().to_string_lossy().to_string(),
        None => return Some((String::new(), PathBuf::new())),
    };
    let remainder: PathBuf = components.collect();
    Some((storage_name, remainder))
}

/// Enumerate all known storage replica set names (local + remote).
///
/// Used by both the placeholder reconciler and the provider's
/// `fetch_placeholders` callback.
pub(crate) async fn enumerate_storage_names(
    volumes: &Volumes,
    registry: &GardenRegistry,
) -> HashSet<String> {
    let mut names = HashSet::new();

    {
        let map = volumes.read().await;
        for vol in map.values() {
            if let Some(ref mgmt) = vol.management {
                let rs_name = if mgmt.replica_set_name.is_empty() {
                    DEFAULT_REPLICA_SET_DISPLAY.to_string()
                } else {
                    mgmt.replica_set_name.clone()
                };
                names.insert(rs_name);
            }
        }
    }

    {
        let reg = registry.read().await;
        for entry in reg.storage_entries() {
            let name = &entry.tool.fqid;
            if !name.is_empty() {
                names.insert(name.clone());
            }
        }
    }

    names
}

// ============================================================================
// Public API
// ============================================================================

/// Start the Cloud Filter sync provider.
///
/// Registers the sync root, connects the filter, and spawns background
/// tasks for placeholder reconciliation and ingest (write-back).
pub async fn start(
    volumes: Volumes,
    registry: GardenRegistry,
    stone_id: String,
    tick_tx: tokio::sync::broadcast::Sender<StorageTick>,
    storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    local_endpoint: Arc<RwLock<String>>,
    shutdown_token: CancellationToken,
) -> Result<()> {
    // Check platform support
    match cloud_filter::root::is_supported() {
        Ok(true) => {}
        Ok(false) => {
            info!("Cloud Filter API not supported on this Windows version");
            return Ok(());
        }
        Err(e) => {
            warn!(error = %e, "Cloud Filter support check failed");
            return Ok(());
        }
    }

    // Log process context for diagnostics
    info!(
        elevated = registration::is_elevated(),
        service = registration::is_running_as_service(),
        username = %std::env::var("USERNAME").unwrap_or_default(),
        "Cloud Filter: process context"
    );

    // Step 1: Ensure sync root is registered
    let sync_root_path = registration::ensure_registered().await?;

    // Step 2: Connect the provider
    let provider = ZenGardenProvider {
        volumes: volumes.clone(),
        registry: registry.clone(),
        stone_id: stone_id.clone(),
        tick_tx,
        sync_root_path: sync_root_path.clone(),
        local_endpoint,
    };

    // CfApi callbacks fire on Windows threadpool threads (not inside a tokio
    // async context), so Handle::block_on is safe here.  We cannot use
    // futures::executor::block_on because our Filter impl uses
    // tokio::sync::RwLock which participates in Tokio's cooperative
    // scheduling — futures::executor wouldn't drive the coop budget and
    // would deadlock on uncontended locks.
    let rt = tokio::runtime::Handle::current();
    let connection = Session::new()
        .connect_async(
            &sync_root_path,
            provider,
            move |future| rt.block_on(future),
        )
        .context("failed to connect Cloud Filter provider")?;

    info!(path = %sync_root_path.display(), "Cloud Filter provider connected");

    // Step 3: Spawn storage watcher (keeps connection alive)
    let watcher_token = shutdown_token.child_token();
    let ingest_volumes = volumes.clone();
    let ingest_sync_root = sync_root_path.clone();
    let ingest_storage_rx = storage_changed_rx.resubscribe();
    tokio::spawn(async move {
        let _connection = connection; // kept alive until shutdown

        storage_watcher(
            volumes,
            registry,
            &sync_root_path,
            storage_changed_rx,
            watcher_token,
        )
        .await;

        info!("Cloud Filter provider disconnected");
    });

    // Step 4: Spawn ingest watcher (write-back from Explorer)
    let ingest_token = shutdown_token.child_token();
    tokio::spawn(async move {
        ingest::run(ingest_volumes, ingest_sync_root, ingest_storage_rx, ingest_token).await;
        info!("Cloud Filter ingest watcher stopped");
    });

    Ok(())
}

/// Unregister the sync root (for clean uninstall).
pub fn unregister() -> Result<()> {
    registration::unregister()
}

// ============================================================================
// Storage watcher task
// ============================================================================

/// Watches `Volumes` + `GardenRegistry` for changes and creates/removes
/// placeholder directories under the sync root.
///
/// Event-driven (STORAGE-0013): reacts immediately to `StorageChanged` events.
/// Falls back to a 60s heartbeat poll for resilience (registry changes from
/// remote stones don't emit local StorageChanged events).
async fn storage_watcher(
    volumes: Volumes,
    registry: GardenRegistry,
    sync_root_path: &Path,
    mut storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    shutdown_token: CancellationToken,
) {
    // Seed `known` from existing placeholder directories so we detect and
    // remove stale entries from previous sessions on the first reconcile pass.
    let mut known = scan_existing_placeholders(sync_root_path).await;
    let heartbeat = tokio::time::Duration::from_secs(60);

    debug!(
        existing = known.len(),
        "storage watcher started (event-driven + 60s heartbeat)"
    );

    // Run initial reconciliation immediately
    reconcile_placeholders(&volumes, &registry, sync_root_path, &mut known).await;

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                debug!("storage watcher shutting down");
                break;
            }
            result = storage_changed_rx.recv() => {
                match result {
                    Ok(event) => {
                        debug!(event = ?event, "storage watcher: event received");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!(skipped = n, "storage watcher: lagged, reconciling");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!("storage watcher: channel closed");
                        break;
                    }
                }
                reconcile_placeholders(&volumes, &registry, sync_root_path, &mut known).await;
            }
            _ = tokio::time::sleep(heartbeat) => {
                reconcile_placeholders(&volumes, &registry, sync_root_path, &mut known).await;
            }
        }
    }
}

/// Reconcile placeholder directories with current storage names.
async fn reconcile_placeholders(
    volumes: &Volumes,
    registry: &GardenRegistry,
    sync_root_path: &Path,
    known: &mut HashSet<String>,
) {
    let current = enumerate_storage_names(volumes, registry).await;

    if current == *known {
        debug!(total = current.len(), "no storage changes");
        return;
    }

    let added: Vec<_> = current.difference(known).cloned().collect();
    let removed: Vec<_> = known.difference(&current).cloned().collect();

    for name in &added {
        placeholders::create_storage_placeholder(sync_root_path, name);
    }
    if !added.is_empty() {
        info!(storages = ?added, "new storages visible in Explorer");
    }

    for name in &removed {
        placeholders::remove_storage_placeholder(sync_root_path, name).await;
    }
    if !removed.is_empty() {
        info!(storages = ?removed, "storages removed from Explorer");
    }

    *known = current;
}

/// Scan existing subdirectories under the sync root to seed `known`.
///
/// Without this, a restart would leave stale placeholder directories from
/// previous storage names — the watcher starts with an empty `known` set
/// and never detects that old names departed.
async fn scan_existing_placeholders(sync_root_path: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut dir = match tokio::fs::read_dir(sync_root_path).await {
        Ok(d) => d,
        Err(e) => {
            debug!(error = %e, "could not scan sync root for existing placeholders");
            return names;
        }
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        if let Ok(ft) = entry.file_type().await {
            if ft.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                names.insert(name);
            }
        }
    }

    names
}
