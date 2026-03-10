//! Cloud Filter integration (STORAGE-0009 Phase 4, rebuilt per STORAGE-0012)
//!
//! Registers a Windows Cloud Sync Provider so managed storages appear
//! natively in Explorer under "Zen Garden".  Files are fetched on demand
//! from the hosting stone's storage API.
//!
//! ## Module layout (STORAGE-0012)
//!
//! - `registration.rs` — sync root registration lifecycle
//! - `provider.rs`     — CfApi `Filter` trait callbacks
//! - `placeholders.rs` — shared placeholder helpers (valid timestamps)
//!
//! ## Lifecycle
//!
//! 1. `start()` ensures the sync root is registered (idempotent)
//! 2. Connects the `ZenGardenProvider` to the sync root
//! 3. Spawns a storage watcher that creates/removes placeholder dirs
//! 4. Spawns an ingest watcher that copies user-pasted files to storage
//! 5. On shutdown, the connection drops (disconnects the provider)

mod placeholders;
mod provider;
mod registration;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use cloud_filter::root::Session;
use notify::Watcher;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use garden_common::storage::{StorageChanged, StorageTick};

use self::provider::ZenGardenProvider;

// ============================================================================
// Public API
// ============================================================================

/// Start the Cloud Filter sync provider.
///
/// Registers the sync root, connects the filter, and spawns a background
/// task that watches the garden registry for storage changes.
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

    // Step 4: Spawn sync root ingest watcher (write-back from Explorer)
    let ingest_token = shutdown_token.child_token();
    tokio::spawn(async move {
        sync_root_ingest_watcher(ingest_volumes, ingest_sync_root, ingest_token).await;
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
    let current = collect_storage_names(volumes, registry).await;

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

/// Collect the set of all storage replica set names (local managed + remote registry).
async fn collect_storage_names(volumes: &Volumes, registry: &GardenRegistry) -> HashSet<String> {
    let mut names = HashSet::new();

    {
        let map = volumes.read().await;
        for vol in map.values() {
            if let Some(ref mgmt) = vol.management {
                let rs_name = if mgmt.replica_set_name.is_empty() {
                    garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY.to_string()
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
// Sync root ingest watcher (write-back from Explorer)
// ============================================================================

/// Watch the sync root for user-created files and copy them to the actual
/// storage mount (write-back path for Cloud Filter).
///
/// Without this, files pasted into the Explorer "Zen Garden" folder sit at
/// the sync root with a "sync pending" overlay but never reach the storage
/// mount — the existing filesystem watcher only monitors the mount path.
async fn sync_root_ingest_watcher(
    volumes: Volumes,
    sync_root_path: PathBuf,
    shutdown_token: CancellationToken,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Event>(256);

    let watch_path = sync_root_path.clone();
    let _watcher = match spawn_sync_root_notify_watcher(watch_path, tx) {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "failed to start sync root ingest watcher");
            return;
        }
    };

    let debounce = tokio::time::Duration::from_secs(2);
    let mut pending: HashMap<PathBuf, ()> = HashMap::new();
    let mut debounce_deadline = tokio::time::Instant::now() + debounce;

    info!("sync root ingest watcher started");

    // Initial scan — catch files pasted before the watcher started
    initial_ingest_scan(&volumes, &sync_root_path).await;

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                debug!("sync root ingest watcher shutting down");
                break;
            }
            event = rx.recv() => {
                let Some(event) = event else { break; };

                // Only handle Create and Modify events (new or changed files)
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
            _ = tokio::time::sleep_until(debounce_deadline) => {
                if pending.is_empty() {
                    debounce_deadline = tokio::time::Instant::now() + debounce;
                    continue;
                }

                let batch: Vec<PathBuf> = pending.drain().map(|(p, _)| p).collect();
                ingest_sync_root_files(&volumes, &sync_root_path, &batch).await;

                debounce_deadline = tokio::time::Instant::now() + debounce;
            }
        }
    }
}

/// Check if a path should be considered for ingestion from the sync root.
///
/// Filters out:
/// - Top-level directories (storage name placeholders — managed separately)
/// - `.zen-garden` metadata directories at any depth
/// - Dehydrated CfApi placeholders (have `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`)
fn should_ingest(path: &Path, sync_root_path: &Path) -> bool {
    let rel = match path.strip_prefix(sync_root_path) {
        Ok(r) => r,
        Err(_) => return false,
    };

    // Need at least storage_name/relative_path (2+ components)
    let components: Vec<_> = rel.components().collect();
    if components.len() < 2 {
        return false;
    }

    // Skip .zen-garden metadata at any depth
    for c in &components {
        if let std::path::Component::Normal(s) = c {
            let s = s.to_string_lossy();
            if s == ".zen-garden" || s == "Zen Garden" {
                return false;
            }
        }
    }

    // Skip dehydrated CfApi placeholders — reading them would trigger
    // fetch_data (hydration loop).  Real user-pasted files don't have
    // the recall attribute.
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.file_attributes() & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS != 0 {
                return false;
            }
        }
    }

    true
}

/// Process a batch of sync root paths, copying new files to the storage mount.
async fn ingest_sync_root_files(
    volumes: &Volumes,
    sync_root_path: &Path,
    paths: &[PathBuf],
) {
    let mut ingested = 0u32;

    for path in paths {
        let rel = match path.strip_prefix(sync_root_path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut components = rel.components();
        let storage_name = match components.next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => continue,
        };
        let remainder: PathBuf = components.collect();
        if remainder.as_os_str().is_empty() {
            continue;
        }

        // Resolve local mount path for this storage (by replica set name)
        let mount_path = resolve_local_mount(volumes, &storage_name).await;
        let Some(mount_path) = mount_path else {
            debug!(storage = %storage_name, "no local mount for ingest, skipping");
            continue;
        };

        let target = mount_path.join(&remainder);

        // Already exists at the mount — nothing to do
        if target.exists() {
            continue;
        }

        if path.is_dir() {
            if let Err(e) = tokio::fs::create_dir_all(&target).await {
                warn!(
                    storage = %storage_name,
                    path = %remainder.display(),
                    error = %e,
                    "ingest: failed to create directory in storage mount"
                );
            } else {
                debug!(
                    storage = %storage_name,
                    path = %remainder.display(),
                    "ingest: created directory in storage mount"
                );
                ingested += 1;
            }
        } else if path.is_file() {
            if let Some(parent) = target.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::copy(path, &target).await {
                Ok(bytes) => {
                    debug!(
                        storage = %storage_name,
                        path = %remainder.display(),
                        bytes,
                        "ingest: copied file to storage mount"
                    );
                    ingested += 1;
                }
                Err(e) => {
                    warn!(
                        storage = %storage_name,
                        path = %remainder.display(),
                        error = %e,
                        "ingest: failed to copy file to storage mount"
                    );
                }
            }
        }
    }

    if ingested > 0 {
        info!(count = ingested, "ingested files from Explorer to storage");
    }
}

/// Initial scan: find files in the sync root that don't exist at the
/// storage mount.  Covers files pasted before the watcher started.
async fn initial_ingest_scan(volumes: &Volumes, sync_root_path: &Path) {
    let mut pending = Vec::new();
    let mut dirs_to_scan = vec![sync_root_path.to_path_buf()];

    while let Some(dir_path) = dirs_to_scan.pop() {
        let mut dir = match tokio::fs::read_dir(&dir_path).await {
            Ok(d) => d,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip managed metadata directories
            if name_str == ".zen-garden" || name_str == "Zen Garden" {
                continue;
            }

            let is_dir = entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false);

            if should_ingest(&path, sync_root_path) {
                pending.push(path.clone());
            }

            if is_dir {
                dirs_to_scan.push(path);
            }
        }
    }

    if !pending.is_empty() {
        debug!(count = pending.len(), "initial ingest scan found files to sync");
        ingest_sync_root_files(volumes, sync_root_path, &pending).await;
    }
}

/// Look up the local mount path for a storage by its replica set name.
async fn resolve_local_mount(volumes: &Volumes, storage_name: &str) -> Option<PathBuf> {
    let map = volumes.read().await;
    map.values().find_map(|v| {
        let mgmt = v.management.as_ref()?;
        let rs_name = if mgmt.replica_set_name.is_empty() {
            garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &mgmt.replica_set_name
        };
        if rs_name == storage_name {
            Some(v.mount_path.clone())
        } else {
            None
        }
    })
}

/// Spawn a `notify` watcher on the sync root directory.
fn spawn_sync_root_notify_watcher(
    path: PathBuf,
    tx: tokio::sync::mpsc::Sender<notify::Event>,
) -> Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                let _ = tx.try_send(event);
            }
            Err(e) => {
                warn!(error = %e, "sync root ingest watcher error");
            }
        },
    )
    .context("failed to create sync root ingest watcher")?;

    watcher
        .watch(&path, notify::RecursiveMode::Recursive)
        .context("failed to watch sync root for ingestion")?;

    Ok(watcher)
}
