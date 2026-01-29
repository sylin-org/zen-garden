//! Storage beacon broadcasting
//!
//! Broadcasts STORAGE_BEACON announcements to notify other stones
//! about this stone's storage capabilities.
//!
//! See docs/decisions/STORAGE-0003-beacon-protocol.md

use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::storage::{SeedBankAnnouncement, StorageBeacon};
use tracing::{debug, info, warn};

use super::SeedBankRegistry;
use crate::domain::storage_cache::StorageCache;

/// Build a storage beacon for this stone
pub async fn build_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
) -> Result<StorageBeacon> {
    // Scan current seed banks
    let registry = SeedBankRegistry::scan().await
        .context("Failed to scan seed banks")?;

    let seed_banks: Vec<SeedBankAnnouncement> = registry
        .list()
        .iter()
        .map(|info| SeedBankAnnouncement::from_info(info))
        .collect();

    Ok(StorageBeacon {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        endpoint: endpoint.to_string(),
        seed_banks,
        timestamp: Utc::now(),
    })
}

/// Broadcast a storage beacon to all stones
///
/// Called on:
/// - Seed bank mount/unmount
/// - Visibility change
/// - Stone online (triggered by STONE_CHIRP from new stone)
pub async fn broadcast_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
) -> Result<()> {
    let beacon = build_beacon(stone_id, stone_name, endpoint).await?;

    let seed_bank_count = beacon.seed_banks.len();

    // Skip broadcast if no storage capability (reduces noise)
    // Exception: Always broadcast if called explicitly (e.g., mount → empty after unmount)
    debug!(
        stone = %stone_name,
        seed_banks = seed_bank_count,
        "Broadcasting storage beacon"
    );

    p2p::send_announcement(announcement_types::STORAGE_BEACON, &beacon)
        .await
        .context("Failed to send STORAGE_BEACON")?;

    info!(
        stone = %stone_name,
        seed_banks = seed_bank_count,
        "Storage beacon broadcast complete"
    );

    Ok(())
}

/// Broadcast beacon if this stone has storage
///
/// Use this when responding to a new stone coming online.
/// Only broadcasts if local stone has seed banks.
pub async fn broadcast_if_has_storage(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
) -> Result<bool> {
    // Quick check if we have any storage
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to scan seed banks for beacon check");
            return Ok(false);
        }
    };

    if registry.list().is_empty() {
        debug!(stone = %stone_name, "No seed banks, skipping beacon");
        return Ok(false);
    }

    // We have storage, broadcast beacon
    broadcast_beacon(stone_id, stone_name, endpoint).await?;
    Ok(true)
}

/// Update local storage cache with this stone's storage
///
/// Called at startup and after mount/unmount events to ensure storage_cache
/// reflects the local stone's storage capabilities. This makes storage_cache
/// the unified view for both local and remote storage.
///
/// Does NOT broadcast a beacon - use broadcast_beacon for that.
pub async fn update_local_storage_cache(
    storage_cache: &StorageCache,
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
) -> Result<()> {
    let beacon = build_beacon(stone_id, stone_name, endpoint).await?;
    
    debug!(
        stone = %stone_name,
        seed_banks = beacon.seed_banks.len(),
        "Updating local storage cache"
    );
    
    crate::domain::storage_cache::update_from_beacon(storage_cache, beacon).await;
    Ok(())
}

/// Update local storage cache AND broadcast beacon to network
///
/// Convenience function for mount/unmount events where both local cache
/// and network need to be updated.
pub async fn update_and_broadcast(
    storage_cache: &StorageCache,
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
) -> Result<()> {
    // Update local cache
    update_local_storage_cache(storage_cache, stone_id, stone_name, endpoint).await?;
    
    // Broadcast to network
    broadcast_beacon(stone_id, stone_name, endpoint).await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_structure() {
        // Just verify the beacon types are correctly importable
        let beacon = StorageBeacon::empty("test-id", "test-name", "http://test:7185");
        assert!(!beacon.has_storage());
        assert!(!beacon.supports_s3());
    }
}
