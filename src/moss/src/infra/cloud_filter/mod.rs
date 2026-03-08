//! Cloud Filter integration (STORAGE-0009 Phase 4)
//!
//! Registers a Windows Cloud Sync Provider so managed storages appear
//! natively in Explorer. Files are fetched on demand from the hosting
//! stone's storage API; saves push back to the Primary.
//!
//! ## Lifecycle
//!
//! 1. `start()` registers the "Zen Garden" sync root (idempotent)
//! 2. A background task watches the `GardenRegistry` for storage
//!    beacons and creates/removes top-level placeholder directories
//! 3. The `ZenGardenProvider` implements Cloud Filter's `Filter` trait,
//!    serving `fetch_data` and `fetch_placeholders` callbacks
//! 4. On shutdown, the connection is dropped (disconnects the provider)
//!
//! ## Architecture
//!
//! Thin infra layer — delegates all I/O to `StorageService` (domain).
//! No business logic here, just the CfApi adapter.

mod provider;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use cloud_filter::root::{HydrationType, PopulationType, Session, SyncRootId, SyncRootIdBuilder, SyncRootInfo};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::domain::garden_registry::GardenRegistry;
use crate::domain::managed_storage::ManagedStorages;
use garden_common::storage::StorageTick;

use self::provider::ZenGardenProvider;

// ============================================================================
// Constants
// ============================================================================

/// Provider name registered with Windows.
const PROVIDER_NAME: &str = "ZenGarden";

/// Display name shown in Explorer's navigation pane.
const DISPLAY_NAME: &str = "Zen Garden";

/// Folder name under the user's home directory.
const SYNC_ROOT_FOLDER: &str = "Zen Garden";

// ============================================================================
// Public API
// ============================================================================

/// Start the Cloud Filter sync provider.
///
/// Registers the sync root, connects the filter, and spawns a background
/// task that watches the garden registry for storage changes.
///
/// Returns a `CancellationToken` child — cancel to shut down.
pub async fn start(
    managed_storages: ManagedStorages,
    registry: GardenRegistry,
    stone_id: String,
    tick_tx: tokio::sync::broadcast::Sender<StorageTick>,
    local_endpoint: Arc<RwLock<String>>,
    shutdown_token: CancellationToken,
) -> Result<()> {
    // Check platform support
    if !cloud_filter::root::is_supported().unwrap_or(false) {
        warn!("Cloud Filter API not supported on this Windows version (requires 1709+)");
        return Ok(());
    }

    let sync_root_path = default_sync_root_path()?;

    // Ensure sync root directory exists
    tokio::fs::create_dir_all(&sync_root_path)
        .await
        .context("Failed to create sync root directory")?;

    // Register sync root (idempotent)
    register_sync_root(&sync_root_path)?;

    info!(path = %sync_root_path.display(), "Cloud Filter sync root registered");

    // Build the provider
    let provider = ZenGardenProvider {
        managed_storages: managed_storages.clone(),
        registry: registry.clone(),
        stone_id: stone_id.clone(),
        tick_tx,
        sync_root_path: sync_root_path.clone(),
        local_endpoint,
    };

    // Connect to CfApi — the connection must stay alive
    let rt = tokio::runtime::Handle::current();
    let connection = Session::new()
        .connect_async(
            &sync_root_path,
            provider,
            move |future| rt.block_on(future),
        )
        .context("Failed to connect Cloud Filter provider")?;

    info!("Cloud Filter provider connected");

    // Spawn storage watcher task
    let watcher_token = shutdown_token.child_token();
    let watcher_storages = managed_storages.clone();
    let watcher_registry = registry.clone();
    let watcher_root = sync_root_path.clone();
    let watcher_stone_id = stone_id.clone();

    tokio::spawn(async move {
        // Keep connection alive until shutdown
        let _connection = connection;

        storage_watcher_task(
            watcher_storages,
            watcher_registry,
            &watcher_stone_id,
            &watcher_root,
            watcher_token,
        )
        .await;

        // Connection drops here, disconnecting the provider
        info!("Cloud Filter provider disconnected");
    });

    Ok(())
}

// ============================================================================
// Sync root registration
// ============================================================================

/// Determine the default sync root path: `%USERPROFILE%\Zen Garden\`
fn default_sync_root_path() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("Could not determine user home directory")?;
    Ok(PathBuf::from(home).join(SYNC_ROOT_FOLDER))
}

/// Build the sync root ID for this provider + user.
fn build_sync_root_id() -> Result<SyncRootId> {
    let sid = cloud_filter::root::SecurityId::current_user()
        .context("Failed to get current user SID")?;

    Ok(SyncRootIdBuilder::new(PROVIDER_NAME)
        .user_security_id(sid)
        .build())
}

/// Register the sync root with Windows (idempotent).
fn register_sync_root(path: &Path) -> Result<()> {
    let sync_root_id = build_sync_root_id()?;

    if sync_root_id.is_registered().unwrap_or(false) {
        debug!("Cloud Filter sync root already registered");
        return Ok(());
    }

    let info = SyncRootInfo::default()
        .with_display_name(DISPLAY_NAME)
        .with_hydration_type(HydrationType::Full)
        .with_population_type(PopulationType::Full)
        .with_path(path)
        .context("Failed to set sync root path")?;

    sync_root_id
        .register(info)
        .context("Failed to register Cloud Filter sync root")?;

    Ok(())
}

// ============================================================================
// Storage watcher task
// ============================================================================

/// Background task that watches for storage changes and creates/removes
/// top-level placeholder directories under the sync root.
///
/// Polls every 10 seconds — matches the storage beacon cadence.
async fn storage_watcher_task(
    managed_storages: ManagedStorages,
    registry: GardenRegistry,
    stone_id: &str,
    sync_root_path: &Path,
    shutdown_token: CancellationToken,
) {
    let mut known_storages: HashSet<String> = HashSet::new();
    let poll_interval = tokio::time::Duration::from_secs(10);

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                debug!("Cloud Filter storage watcher shutting down");
                break;
            }
            _ = tokio::time::sleep(poll_interval) => {
                if let Err(e) = reconcile_storage_folders(
                    &managed_storages,
                    &registry,
                    stone_id,
                    sync_root_path,
                    &mut known_storages,
                ).await {
                    warn!(error = %e, "Cloud Filter: failed to reconcile storage folders");
                }
            }
        }
    }
}

/// Reconcile sync root subdirectories with the current set of known storages.
///
/// Creates directories for new storages, removes directories for departed ones.
async fn reconcile_storage_folders(
    managed_storages: &ManagedStorages,
    registry: &GardenRegistry,
    _stone_id: &str,
    sync_root_path: &Path,
    known: &mut HashSet<String>,
) -> Result<()> {
    let mut current: HashSet<String> = HashSet::new();

    // Collect local storage names
    {
        let banks = managed_storages.read().await;
        for bank in banks.values() {
            current.insert(bank.name.clone());
        }
    }

    // Collect remote storage names from registry
    {
        let reg = registry.read().await;
        for entry in reg.storage_entries() {
            current.insert(entry.tool.tool.name.clone());
        }
    }

    // Create directories for new storages
    for name in &current {
        if !known.contains(name) {
            let dir = sync_root_path.join(name);
            if !dir.exists() {
                if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                    warn!(storage = %name, error = %e, "Cloud Filter: failed to create storage folder");
                } else {
                    info!(storage = %name, "Cloud Filter: storage folder created in sync root");
                }
            }
            known.insert(name.clone());
        }
    }

    // Remove directories for departed storages
    let departed: Vec<String> = known
        .iter()
        .filter(|name| !current.contains(*name))
        .cloned()
        .collect();

    for name in departed {
        let dir = sync_root_path.join(&name);
        if dir.exists() {
            // Only remove if empty (don't delete user data)
            if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
                if entries.next_entry().await.ok().flatten().is_none() {
                    let _ = tokio::fs::remove_dir(&dir).await;
                    info!(storage = %name, "Cloud Filter: empty storage folder removed");
                } else {
                    debug!(storage = %name, "Cloud Filter: storage departed but folder not empty, keeping");
                }
            }
        }
        known.remove(&name);
    }

    Ok(())
}

// ============================================================================
// Cleanup
// ============================================================================

/// Unregister the sync root (for clean uninstall).
pub fn unregister() -> Result<()> {
    let sync_root_id = build_sync_root_id()?;
    if sync_root_id.is_registered().unwrap_or(false) {
        sync_root_id
            .unregister()
            .context("Failed to unregister Cloud Filter sync root")?;
        info!("Cloud Filter sync root unregistered");
    }
    Ok(())
}
