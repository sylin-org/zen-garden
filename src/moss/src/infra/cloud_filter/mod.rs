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
//! The storage watcher maintains two orthogonal views of storage state:
//!
//! ### Set-level (WinRT toast)
//! Tracks the number of ready members per replica set.  A toast fires only
//! when a set crosses the 0 ↔ 1 member boundary — adding a second replica
//! or removing a non-last replica is silent.
//!
//! ### Per-storage (console)
//! Tracks individual managed storages (one entry per stone per set).
//! A console event fires on every individual storage appearing or departing,
//! regardless of set-level state.

mod ingest;
pub(crate) mod placeholders;
mod provider;
mod registration;
mod signaling;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use cloud_filter::root::Session;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use garden_common::console::ConsolePrinter;
use garden_common::storage::{StorageChanged, StorageTick};
use garden_common::tools::ToolDelta;

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

// ============================================================================
// Storage snapshot
// ============================================================================

/// Metadata for one individual managed storage instance (one stone, one set).
struct StorageMember {
    stone_name: String,
}

/// Point-in-time view of all storage state.
///
/// Computed once per reconcile pass and used by both placeholder management
/// and the two notification tiers.
struct StorageSnapshot {
    /// Sets with ≥1 ready member, keyed by fqid (set name).
    ///
    /// Used for placeholder creation and the set-level member count.
    /// Local entries (this stone) take precedence over registry entries for
    /// the same fqid — routing preference only, does not affect counts.
    available_sets: HashMap<String, StorageAvailability>,

    /// All individual ready members, keyed by `(stone_id, fqid)`.
    ///
    /// One entry per stone per set.  Used by the per-storage roster diff to
    /// detect individual storages appearing or departing.
    ready_members: HashMap<(String, String), StorageMember>,

    /// All known set names regardless of ready state.
    ///
    /// Includes registry entries with `ready=false` and offline local volumes.
    /// Used for the "deprovisioned" check: a placeholder is removed only when
    /// a name disappears from this set entirely (not just when offline).
    all_set_names: HashSet<String>,
}

/// Return the set of available storages (≥1 ready member), keyed by fqid.
///
/// Thin wrapper used by the CfApi provider to list storages for fetch routing.
pub(crate) async fn enumerate_storage_availability(
    volumes: &Volumes,
    registry: &GardenRegistry,
    local_stone_id: &str,
) -> HashMap<String, StorageAvailability> {
    snapshot_storage(volumes, registry, local_stone_id).await.available_sets
}

async fn snapshot_storage(
    volumes: &Volumes,
    registry: &GardenRegistry,
    local_stone_id: &str,
) -> StorageSnapshot {
    let mut available_sets: HashMap<String, StorageAvailability> = HashMap::new();
    let mut ready_members: HashMap<(String, String), StorageMember> = HashMap::new();
    let mut all_set_names: HashSet<String> = HashSet::new();

    // ── Local volumes ────────────────────────────────────────────────────────
    {
        let map = volumes.read().await;
        for vol in map.values() {
            if let Some(ref mgmt) = vol.management {
                let fqid = mgmt.display_name().to_string();
                all_set_names.insert(fqid.clone());
                if vol.online {
                    available_sets
                        .entry(fqid.clone())
                        .or_insert_with(|| StorageAvailability::online("this device", true));
                    ready_members.insert(
                        (local_stone_id.to_string(), fqid.clone()),
                        StorageMember { stone_name: "this device".to_string() },
                    );
                }
            }
        }
    }

    // ── Registry entries (remote stones) ────────────────────────────────────
    // Local stone is authoritative for its own storage — skip local entries
    // here so a stale registry value cannot override the volumes loop above.
    {
        let reg = registry.read().await;
        for entry in reg.storage_entries() {
            let fqid = &entry.tool.fqid;
            if fqid.is_empty() {
                continue;
            }

            // Local stone: volumes loop is authoritative; registry may lag.
            if entry.tool.stone.id == local_stone_id {
                continue;
            }

            // All entries (online or not) count toward known names.
            all_set_names.insert(fqid.clone());

            if !entry.tool.service.ready {
                // `ready=false`: storage offline on its stone.  Still in
                // all_set_names so we keep the placeholder (not deprovisioned),
                // but not in available_sets or ready_members.
                continue;
            }

            // available_sets: local entry wins for routing (already inserted
            // by the volumes loop above if this stone also has the set).
            if !available_sets.contains_key(fqid.as_str()) {
                let local = entry.tool.stone.id == local_stone_id;
                available_sets.insert(
                    fqid.clone(),
                    StorageAvailability::online(entry.tool.stone.name.clone(), local),
                );
            }

            // ready_members: one entry per stone — always insert, even if
            // available_sets already has a local entry for this fqid.
            ready_members.insert(
                (entry.tool.stone.id.clone(), fqid.clone()),
                StorageMember { stone_name: entry.tool.stone.name.clone() },
            );
        }
    }

    StorageSnapshot { available_sets, ready_members, all_set_names }
}

// ============================================================================
// Public API
// ============================================================================

/// Start the Cloud Filter sync provider.
///
/// Registers the sync root, connects the filter, and spawns background
/// tasks for placeholder reconciliation and ingest (write-back).
#[allow(clippy::too_many_arguments)]
pub async fn start(
    volumes: Volumes,
    registry: GardenRegistry,
    stone_id: String,
    tick_tx: tokio::sync::broadcast::Sender<StorageTick>,
    storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    tool_delta_rx: tokio::sync::broadcast::Receiver<ToolDelta>,
    console: Arc<ConsolePrinter>,
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

    // Step 1: Ensure sync root is registered + AUMID for toast notifications
    let sync_root_path = registration::ensure_registered().await?;
    signaling::init();

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
    let watcher_console = console.clone();
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
            tool_delta_rx,
            watcher_console,
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
/// Three trigger sources:
/// - `StorageChanged` — local volume add/remove (immediate)
/// - `ToolDelta` (storage category) — remote stone storage appearing or
///   departing in the garden registry (immediate, no heartbeat lag)
/// - 60s heartbeat — resilience catch-all for any missed events
#[allow(clippy::too_many_arguments)]
async fn storage_watcher(
    volumes: Volumes,
    registry: GardenRegistry,
    sync_root_path: &Path,
    stone_id: String,
    mut storage_changed_rx: tokio::sync::broadcast::Receiver<StorageChanged>,
    mut tool_delta_rx: tokio::sync::broadcast::Receiver<ToolDelta>,
    console: Arc<ConsolePrinter>,
    shutdown_token: CancellationToken,
) {
    // Seed `known` from existing placeholder directories so we detect and
    // remove stale entries from previous sessions on the first reconcile pass.
    let mut known = scan_existing_placeholders(sync_root_path).await;

    // Set-level: member count per set name (fqid).  Used for toast 0↔1 boundary.
    // Not initialised from `known` — the startup reconcile seeds it without
    // firing any notifications (notify=false).
    let mut set_counts: HashMap<String, usize> = HashMap::new();

    // Per-storage roster: (stone_id, fqid) → stone_name.  Used for console
    // events on individual storage appearances/departures.
    let mut roster: HashMap<(String, String), StorageMember> = HashMap::new();

    let heartbeat = tokio::time::Duration::from_secs(60);

    debug!(
        existing = known.len(),
        "storage watcher started (event-driven + 60s heartbeat)"
    );

    // Startup reconcile: seed state without notifications.  The registry is
    // still empty (tools beacons haven't arrived yet), so this mostly just
    // records what's already on disk.  No toasts or console events fire here —
    // only genuine user actions after startup produce feedback.
    reconcile_placeholders(
        &volumes,
        &registry,
        sync_root_path,
        &stone_id,
        &mut known,
        &mut set_counts,
        &mut roster,
        &console,
        false, // purge_strays
        false, // notify
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
                    Ok(event) => debug!(event = ?event, "storage watcher: local event"),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!(skipped = n, "storage watcher: storage_changed lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!("storage watcher: storage_changed channel closed");
                        break;
                    }
                }
                reconcile_placeholders(
                    &volumes, &registry, sync_root_path, &stone_id,
                    &mut known, &mut set_counts, &mut roster, &console,
                    false, // purge_strays: avoid racing with ingest
                    true,  // notify
                )
                .await;
            }
            result = tool_delta_rx.recv() => {
                // React immediately when a remote stone's storage entry
                // appears or disappears.  Filter to storage-category deltas
                // only — offering churn is frequent and irrelevant here.
                let is_storage = match &result {
                    Ok(delta) => delta.tool_key.ends_with(":storage")
                        || delta.tool.as_ref().is_some_and(|t| {
                            t.tool.category == garden_common::constants::CATEGORY_STORAGE
                        }),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!("storage watcher: tool_delta channel closed");
                        break;
                    }
                };
                if is_storage {
                    debug!("storage watcher: remote storage delta — reconciling");
                    reconcile_placeholders(
                        &volumes, &registry, sync_root_path, &stone_id,
                        &mut known, &mut set_counts, &mut roster, &console,
                        false, // purge_strays
                        true,  // notify
                    )
                    .await;
                }
            }
            _ = tokio::time::sleep(heartbeat) => {
                reconcile_placeholders(
                    &volumes, &registry, sync_root_path, &stone_id,
                    &mut known, &mut set_counts, &mut roster, &console,
                    true, // purge_strays: heartbeat does full cleanup
                    true, // notify
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
/// ### Placeholder management
/// - **New sets** (not in `known`): create placeholder with correct IN_SYNC state.
/// - **Deprovisioned sets** (gone from all sources): remove placeholder.
/// - **IN_SYNC flag**: updated for every known set on every pass.
///
/// ### Set-level notifications (`set_counts`)
/// Fires toasts only when a set crosses the 0 ↔ 1 member boundary.
///
/// ### Per-storage notifications (`roster`)
/// Fires console events when any individual `(stone_id, fqid)` pair appears
/// or disappears from the set of ready members.
///
/// ### `notify`
/// Pass `false` on the startup reconcile to seed state without firing any
/// events.  Pass `true` for all event-driven and heartbeat passes.
#[allow(clippy::too_many_arguments)]
async fn reconcile_placeholders(
    volumes: &Volumes,
    registry: &GardenRegistry,
    sync_root_path: &Path,
    stone_id: &str,
    known: &mut HashSet<String>,
    set_counts: &mut HashMap<String, usize>,
    roster: &mut HashMap<(String, String), StorageMember>,
    console: &Arc<ConsolePrinter>,
    purge_strays: bool,
    notify: bool,
) {
    if purge_strays {
        purge_blocked_placeholders(sync_root_path).await;
    }

    let snap = snapshot_storage(volumes, registry, stone_id).await;
    let current_online: HashSet<String> = snap.available_sets.keys().cloned().collect();

    if purge_strays {
        purge_stray_root_items(sync_root_path, known).await;
    }

    // ── Add new sets ─────────────────────────────────────────────────────────

    let added: Vec<String> = current_online.difference(known).cloned().collect();
    for name in &added {
        let avail = &snap.available_sets[name];
        placeholders::create_storage_placeholder(sync_root_path, name, avail);
        known.insert(name.clone());
    }
    if !added.is_empty() {
        info!(sets = ?added, "new storage sets visible in Explorer");
    }

    // ── Remove deprovisioned sets ────────────────────────────────────────────
    //
    // A set is deprovisioned when it disappears from ALL sources: not in local
    // volumes (even offline) and not in the registry.  Offline sets still have
    // a registry entry (ready=false) so their placeholder is kept.

    let removed: Vec<String> = known
        .iter()
        .filter(|n| !snap.all_set_names.contains(*n))
        .cloned()
        .collect();
    for name in &removed {
        placeholders::remove_storage_placeholder(sync_root_path, name).await;
        known.remove(name);
        // Remove from set_counts so the set is treated as brand-new if it
        // returns later (fires set_connected rather than set_returned).
        set_counts.remove(name);
    }
    if !removed.is_empty() {
        info!(sets = ?removed, "storage sets removed from Explorer");
    }

    // ── Update IN_SYNC for all known sets ────────────────────────────────────

    for name in known.iter() {
        let online = current_online.contains(name);
        placeholders::update_storage_placeholder_state(sync_root_path, name, online);
    }

    // ── Set-level notifications (toasts) ─────────────────────────────────────
    //
    // For each set: compute new member count, compare to previous, fire toast
    // only on the 0 ↔ 1 boundary.

    // Count ready members per set from the snapshot.
    let mut new_counts: HashMap<String, usize> = HashMap::new();
    for (_, fqid) in snap.ready_members.keys() {
        *new_counts.entry(fqid.clone()).or_insert(0) += 1;
    }

    // Union of previously tracked sets and newly discovered sets.
    let all_set_keys: Vec<String> = {
        let mut keys: HashSet<String> = new_counts.keys().cloned().collect();
        keys.extend(set_counts.keys().cloned());
        keys.into_iter().collect()
    };

    for name in &all_set_keys {
        let prev = set_counts.get(name.as_str()).copied().unwrap_or(0);
        let next = new_counts.get(name.as_str()).copied().unwrap_or(0);

        if notify {
            match (prev, next) {
                (0, n) if n > 0 => {
                    // 0 → 1: was the set ever tracked before?
                    if set_counts.contains_key(name.as_str()) {
                        signaling::set_returned(name); // was offline, now back
                    } else {
                        signaling::set_connected(name); // first appearance
                    }
                }
                (p, 0) if p > 0 => {
                    signaling::set_offline(name); // lost last member
                }
                _ => {} // count changed but not at boundary, or no change
            }
        }

        set_counts.insert(name.clone(), next);
    }

    // ── Explorer info bar ────────────────────────────────────────────────────
    //
    // Show offline sets that still have a placeholder (i.e. are known but have
    // no ready members).

    let offline_names: Vec<String> = known
        .iter()
        .filter(|n| !current_online.contains(*n))
        .cloned()
        .collect();
    let offline_refs: Vec<&str> = offline_names.iter().map(|s| s.as_str()).collect();

    if !offline_refs.is_empty() {
        signaling::report_sync_status(sync_root_path, &offline_refs);
    } else {
        signaling::clear_sync_status(sync_root_path);
    }

    // ── Per-storage roster (console events) ──────────────────────────────────

    // Collect keys only (owned) so borrows on snap and roster don't overlap.
    let arrived_keys: Vec<(String, String)> = snap
        .ready_members
        .keys()
        .filter(|k| !roster.contains_key(*k))
        .cloned()
        .collect();

    let departed_keys: Vec<(String, String)> = roster
        .keys()
        .filter(|k| !snap.ready_members.contains_key(*k))
        .cloned()
        .collect();

    if notify {
        for key in &arrived_keys {
            if let Some(member) = snap.ready_members.get(key) {
                signaling::storage_available(&key.1, &member.stone_name, console);
            }
        }
        for key in &departed_keys {
            if let Some(member) = roster.get(key) {
                signaling::storage_unavailable(&key.1, &member.stone_name, console);
            }
        }
    }

    // Update roster to exactly match current ready members.
    for key in &departed_keys {
        roster.remove(key);
    }
    for key in arrived_keys {
        if let Some(member) = snap.ready_members.get(&key) {
            roster.insert(key, StorageMember { stone_name: member.stone_name.clone() });
        }
    }

    if added.is_empty() && removed.is_empty() {
        debug!(known = known.len(), "reconcile: no set changes");
    }
}

// ============================================================================
// Maintenance helpers
// ============================================================================

/// Remove blocked-name placeholders from inside each storage subdirectory.
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
/// `known_sets` includes offline local-volume sets so we don't accidentally
/// delete a placeholder for an ejected USB drive.
async fn purge_stray_root_items(sync_root_path: &Path, known_sets: &HashSet<String>) {
    let mut rd = match tokio::fs::read_dir(sync_root_path).await {
        Ok(d) => d,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        if known_sets.contains(&name) {
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
