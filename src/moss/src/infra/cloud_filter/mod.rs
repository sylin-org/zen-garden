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
//! 4. On shutdown, the connection drops (disconnects the provider)

mod placeholders;
mod provider;
mod registration;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use cloud_filter::root::Session;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::storage::Volumes;
use garden_common::storage::StorageTick;

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
    tokio::spawn(async move {
        let _connection = connection; // kept alive until shutdown

        storage_watcher(
            volumes,
            registry,
            &sync_root_path,
            watcher_token,
        )
        .await;

        info!("Cloud Filter provider disconnected");
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
/// Polls every 10 seconds.  Uses `placeholders::create_storage_placeholder`
/// (with valid timestamps) for additions.
async fn storage_watcher(
    volumes: Volumes,
    registry: GardenRegistry,
    sync_root_path: &Path,
    shutdown_token: CancellationToken,
) {
    // Seed `known` from existing placeholder directories so we detect and
    // remove stale entries from previous sessions on the first reconcile pass.
    let mut known = scan_existing_placeholders(sync_root_path).await;
    let poll_interval = tokio::time::Duration::from_secs(10);

    debug!(
        existing = known.len(),
        "storage watcher started (poll every 10s)"
    );

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                debug!("storage watcher shutting down");
                break;
            }
            _ = tokio::time::sleep(poll_interval) => {
                let current = collect_storage_names(&volumes, &registry).await;

                if current == known {
                    debug!(total = current.len(), "no storage changes");
                    continue;
                }

                let added: Vec<_> = current.difference(&known).cloned().collect();
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

                known = current;
            }
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
                names.insert(name);
            }
        }
    }

    names
}

/// Collect the set of all storage names (local managed + remote registry).
async fn collect_storage_names(volumes: &Volumes, registry: &GardenRegistry) -> HashSet<String> {
    let mut names = HashSet::new();

    {
        let map = volumes.read().await;
        for vol in map.values() {
            if let Some(ref mgmt) = vol.management {
                names.insert(mgmt.name.clone());
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
