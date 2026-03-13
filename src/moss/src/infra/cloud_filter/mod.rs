//! Cloud Filter integration (STORAGE-0009 Phase 4, rebuilt per STORAGE-0012,
//! availability signalling per STORAGE-0016)
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
//! - `placeholders.rs` — placeholder helpers + `StorageAvailability`
//! - `signaling.rs`    — Explorer info bar + toast notifications (STORAGE-0016)
//!
//! ## Lifecycle
//!
//! 1. `start()` ensures the sync root is registered (idempotent)
//! 2. Connects the `ZenGardenProvider` to the sync root
//! 3. Spawns a storage watcher that creates/removes placeholder dirs
//! 4. Spawns an ingest watcher that copies user-pasted files to storage
//! 5. On shutdown, the connection drops (disconnects the provider)
//!
//! ## Availability signalling (STORAGE-0016)
//!
//! The storage watcher tracks per-replica-set availability (online/offline)
//! and updates the IN_SYNC flag on each placeholder directory.  On each
//! state transition it fires:
//!
//! - `CfReportSyncStatus` — Explorer info bar listing offline storages
//! - WinRT `ToastNotification` — one-shot desktop alert (suppressed during
//!   the 120 s startup window to avoid cold-boot noise)

mod ingest;
pub(crate) mod placeholders;
mod provider;
mod registration;
mod signaling;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use cloud_filter::root::Session;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use garden_common::storage::{StorageChanged, StorageTick};

use self::placeholders::StorageAvailability;
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

/// Enumerate online storages with availability metadata.
///
/// - Local volumes (`online = true`) → `local = true`, `stone_name = "this device"`
/// - Registry entries (remote stones) → `local = false`, stone name from beacon
///
/// Local entries take precedence when both sources report the same name
/// (this stone broadcasts its own storages).
pub(crate) async fn enumerate_storage_availability(
    volumes: &Volumes,
    registry: &GardenRegistry,
    local_stone_id: &str,
) -> HashMap<String, StorageAvailability> {
    let mut avail: HashMap<String, StorageAvailability> = HashMap::new();

    {
        let map = volumes.read().await;
        for vol in map.values() {
            if !vol.online {
                continue;
            }
            if let Some(ref mgmt) = vol.management {
                let name = mgmt.display_name().to_string();
                avail.insert(name, StorageAvailability::online("this device", true));
            }
        }
    }

    {
        let reg = registry.read().await;
        for entry in reg.storage_entries() {
            let name = &entry.tool.fqid;
            if name.is_empty() {
                continue;
            }
            // Don't override a local entry with a remote one
            if !avail.contains_key(name.as_str()) {
                let local = entry.tool.stone.id == local_stone_id;
                avail.insert(
                    name.clone(),
                    StorageAvailability::online(entry.tool.stone.name.clone(), local),
                );
            }
        }
    }

    avail
}

/// Enumerate ALL storage names — online AND offline local volumes.
///
/// Used by the reconciler to distinguish "storage went offline" (local volume
/// ejected — keep placeholder, mark not-in-sync) from "storage deprovisioned"
/// (volume released and gone from the map — remove placeholder).
///
/// Remote storages that are offline (stone unreachable) are NOT included here
/// because they have no registry entry.  Their placeholders are removed on
/// the next reconcile and recreated when the stone returns.
async fn enumerate_all_storage_names(
    volumes: &Volumes,
    registry: &GardenRegistry,
) -> HashSet<String> {
    let mut names = HashSet::new();

    // All local volumes regardless of online state
    {
        let map = volumes.read().await;
        for vol in map.values() {
            if let Some(ref mgmt) = vol.management {
                names.insert(mgmt.display_name().to_string());
            }
        }
    }

    // Online registry entries (remote stones)
    {
        let reg = registry.read().await;
        for entry in reg.storage_entries() {
            if !entry.tool.fqid.is_empty() {
                names.insert(entry.tool.fqid.clone());
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
    let ingest_tick = tick_tx.clone();
    let provider = ZenGardenProvider {
        volumes: volumes.clone(),
        registry: registry.clone(),
        stone_id: stone_id.clone(),
        tick: tick_tx,
        sync_root_path: sync_root_path.clone(),
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
    let watcher_stone_id = stone_id.clone();
    let ingest_volumes = volumes.clone();
    let ingest_sync_root = sync_root_path.clone();
    let ingest_storage_rx = storage_changed_rx.resubscribe();
    let ingest_registry = registry.clone();
    let ingest_stone_id = stone_id.clone();
    tokio::spawn(async move {
        let _connection = connection; // kept alive until shutdown

        storage_watcher(
            volumes,
            registry,
            &sync_root_path,
            watcher_stone_id,
            storage_changed_rx,
            watcher_token,
        )
        .await;

        info!("Cloud Filter provider disconnected");
    });

    // Step 4: Spawn ingest watcher (write-back from Explorer)
    let ingest_token = shutdown_token.child_token();
    tokio::spawn(async move {
        ingest::run(
            ingest_volumes,
            ingest_registry,
            ingest_stone_id,
            ingest_tick,
            ingest_sync_root,
            ingest_storage_rx,
            ingest_token,
        )
        .await;
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
    stone_id: String,
    mut storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    shutdown_token: CancellationToken,
) {
    // Seed `known` from existing placeholder directories so we detect and
    // remove stale entries from previous sessions on the first reconcile pass.
    let mut known = scan_existing_placeholders(sync_root_path).await;

    // Track per-storage availability state to detect transitions.
    // Seeded as "unknown" (treated as online) so the first reconcile applies
    // the correct state without firing transition notifications.
    let mut prev_avail: HashMap<String, bool> = HashMap::new();

    let heartbeat = tokio::time::Duration::from_secs(60);
    let startup_at = Instant::now();

    debug!(
        existing = known.len(),
        "storage watcher started (event-driven + 60s heartbeat)"
    );

    // Run initial reconciliation without stray purge — the registry is still
    // empty at startup (tools beacons haven't arrived yet).  Purging now would
    // delete legitimate remote-storage placeholders that the heartbeat will
    // rediscover ~60 s later.  The first heartbeat handles stray cleanup once
    // the registry is populated.
    reconcile_placeholders(
        &volumes,
        &registry,
        sync_root_path,
        &stone_id,
        &mut known,
        &mut prev_avail,
        startup_at,
        false,
    )
    .await;

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
                // Event-driven: fast path — skip stray purge to avoid racing
                // with ingest (files pasted by the user are still being processed).
                reconcile_placeholders(
                    &volumes,
                    &registry,
                    sync_root_path,
                    &stone_id,
                    &mut known,
                    &mut prev_avail,
                    startup_at,
                    false,
                )
                .await;
            }
            _ = tokio::time::sleep(heartbeat) => {
                // Heartbeat: full reconciliation including stray purge
                reconcile_placeholders(
                    &volumes,
                    &registry,
                    sync_root_path,
                    &stone_id,
                    &mut known,
                    &mut prev_avail,
                    startup_at,
                    true,
                )
                .await;
            }
        }
    }
}

// ============================================================================
// Reconciliation
// ============================================================================

/// Reconcile placeholder directories with current storage availability.
///
/// - **New** storages (not yet in `known`): create placeholder with correct
///   IN_SYNC state.
/// - **Deprovisioned** storages (gone from both local volumes and registry):
///   remove placeholder.
/// - **Offline local volumes** (ejected USB — still in volumes map but
///   `online = false`): placeholder kept, IN_SYNC cleared.
/// - **State transitions** (online ↔ offline): update IN_SYNC flag, update
///   Explorer info bar via `CfReportSyncStatus`, fire toast notification.
///
/// `purge_strays` controls whether stray root items and blocked placeholders
/// are cleaned up. Set to `true` on heartbeat, `false` on event-driven passes
/// to avoid racing with the ingest watcher (A11c).
#[allow(clippy::too_many_arguments)]
async fn reconcile_placeholders(
    volumes: &Volumes,
    registry: &GardenRegistry,
    sync_root_path: &Path,
    stone_id: &str,
    known: &mut HashSet<String>,
    prev_avail: &mut HashMap<String, bool>,
    startup_at: Instant,
    purge_strays: bool,
) {
    if purge_strays {
        // Remove blocked-name placeholders left over from previous runs
        purge_blocked_placeholders(sync_root_path).await;
    }

    // Current online storages (with stone metadata for blob / notifications)
    let current_avail = enumerate_storage_availability(volumes, registry, stone_id).await;
    let current_online: HashSet<String> = current_avail.keys().cloned().collect();

    // All storage names regardless of online state (local volumes online + offline)
    let all_names = enumerate_all_storage_names(volumes, registry).await;

    if purge_strays {
        // Stray purge uses `known` (includes offline local volumes) rather
        // than `current_online` so we don't accidentally purge a legitimate
        // placeholder for an ejected USB drive.
        purge_stray_root_items(sync_root_path, known).await;
    }

    // ── Add new storages ────────────────────────────────────────────────────

    let added: Vec<String> = current_online.difference(known).cloned().collect();
    for name in &added {
        let avail = &current_avail[name];
        placeholders::create_storage_placeholder(sync_root_path, name, avail);
        known.insert(name.clone());
        prev_avail.insert(name.clone(), true);
    }
    if !added.is_empty() {
        info!(storages = ?added, "new storages visible in Explorer");
    }

    // ── Remove deprovisioned storages ───────────────────────────────────────
    //
    // A storage is deprovisioned when it disappears from ALL sources: not in
    // local volumes (even offline ones) and not in the registry.  Remote
    // storages that are merely offline are transiently absent from the
    // registry; their placeholders are removed and recreated on reconnect.

    let removed: Vec<String> = known
        .iter()
        .filter(|n| !all_names.contains(*n))
        .cloned()
        .collect();
    for name in &removed {
        placeholders::remove_storage_placeholder(sync_root_path, name).await;
        known.remove(name);
        prev_avail.remove(name);
    }
    if !removed.is_empty() {
        info!(storages = ?removed, "storages removed from Explorer");
    }

    // ── Update IN_SYNC for all known storages, detect transitions ───────────

    let mut went_offline: Vec<String> = Vec::new();
    let mut came_online: Vec<(String, String)> = Vec::new(); // (name, stone_name)

    for name in known.iter() {
        let online = current_online.contains(name);
        // Default to `true` so the first pass treats all seeded-from-disk
        // storages as previously online — only genuine offline states at
        // startup update the placeholder without firing notifications.
        let was_online = prev_avail.get(name).copied().unwrap_or(true);

        if online != was_online {
            placeholders::update_storage_placeholder_state(sync_root_path, name, online);
            prev_avail.insert(name.clone(), online);

            if online {
                let stone = current_avail
                    .get(name)
                    .map(|a| a.stone_name.as_str())
                    .unwrap_or("unknown stone");
                came_online.push((name.clone(), stone.to_string()));
            } else {
                went_offline.push(name.clone());
            }
        }
    }

    // ── Phase 3: info bar ───────────────────────────────────────────────────

    let all_offline: Vec<&str> = known
        .iter()
        .filter(|n| !current_online.contains(*n))
        .map(|n| n.as_str())
        .collect();

    if !all_offline.is_empty() {
        signaling::report_sync_status(sync_root_path, &all_offline);
    } else if !went_offline.is_empty() || !came_online.is_empty() {
        // All storages back online — clear the info bar
        signaling::clear_sync_status(sync_root_path);
    }

    // ── Phase 4: toast notifications ────────────────────────────────────────

    for name in &went_offline {
        signaling::notify_offline(name, startup_at);
    }
    for (name, stone_name) in &came_online {
        signaling::notify_online(name, stone_name, startup_at);
    }

    if went_offline.is_empty() && came_online.is_empty() && added.is_empty() && removed.is_empty()
    {
        debug!(total = known.len(), "no storage changes");
    }
}

// ============================================================================
// Maintenance helpers
// ============================================================================

/// Remove blocked-name placeholders from inside each storage subdirectory.
///
/// Cleans up placeholders for system folders (`$RECYCLE.BIN`, etc.) that were
/// created by previous versions before the blocked-name filter existed.
/// Scans `{sync_root}/{storage}/*` — a no-op once all stale entries are gone.
async fn purge_blocked_placeholders(sync_root_path: &Path) {
    let blocked = garden_common::constants::storage::share::blocked_paths();

    let mut rd = match tokio::fs::read_dir(sync_root_path).await {
        Ok(d) => d,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let Ok(ft) = entry.file_type().await else { continue };
        if !ft.is_dir() { continue; }
        let storage_dir = entry.path();
        for &name in blocked {
            let target = storage_dir.join(name);
            if target.exists() {
                match tokio::fs::remove_dir_all(&target).await {
                    Ok(()) => info!(
                        storage = %entry.file_name().to_string_lossy(),
                        name,
                        "purged blocked placeholder from storage"
                    ),
                    Err(e) => debug!(name, error = %e, "could not purge blocked placeholder"),
                }
            }
        }
    }
}

/// Remove stray files and directories at the sync root level.
///
/// CfApi has no CREATE callback, so we cannot prevent users from pasting
/// items directly under the sync root.  This function cleans them up.
///
/// `known_storages` includes both online AND offline local-volume storages so
/// we don't accidentally delete an ejected-USB placeholder.
async fn purge_stray_root_items(sync_root_path: &Path, known_storages: &HashSet<String>) {
    let mut rd = match tokio::fs::read_dir(sync_root_path).await {
        Ok(d) => d,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        if known_storages.contains(&name) {
            continue;
        }

        if garden_common::constants::storage::share::is_blocked_name(&name) {
            continue;
        }

        let Ok(ft) = entry.file_type().await else { continue };
        let path = entry.path();

        let result = if ft.is_dir() {
            tokio::fs::remove_dir_all(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        };

        match result {
            Ok(()) => info!(name = %name, "purged stray item from sync root"),
            Err(e) => debug!(name = %name, error = %e, "could not purge stray root item"),
        }
    }
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
                if !garden_common::constants::storage::share::is_blocked_name(&name) {
                    names.insert(name);
                }
            }
        }
    }

    names
}
