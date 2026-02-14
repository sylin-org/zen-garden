//! Storage routing cache - tracks seed banks across all stones
//!
//! Separate from TopologyCache to keep storage routing concerns decoupled.
//! References TopologyCache by stone_id for liveness checks.
//!
//! See docs/decisions/STORAGE-0003-beacon-protocol.md

use garden_common::storage::{SeedBankAnnouncement, StorageBeacon};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::topology::TopologyCache;

/// In-memory storage routing cache
///
/// Stores storage beacons indexed by stone_id.
/// Updated from STORAGE_BEACON announcements.
pub type StorageCache = Arc<RwLock<StorageCacheInner>>;

/// Create a new empty storage cache
pub fn new_storage_cache() -> StorageCache {
    Arc::new(RwLock::new(StorageCacheInner::default()))
}

/// Inner storage cache data
#[derive(Debug, Default)]
pub struct StorageCacheInner {
    /// stone_id → StorageBeacon (last known state)
    beacons: HashMap<String, StorageBeacon>,
}

impl StorageCacheInner {
    /// Update cache with a new beacon
    pub fn update(&mut self, beacon: StorageBeacon) {
        tracing::debug!(
            stone = %beacon.stone_name,
            seed_banks = beacon.seed_banks.len(),
            "Storage cache updated from beacon"
        );
        self.beacons.insert(beacon.stone_id.clone(), beacon);
    }

    /// Remove a stone's storage entry (when stone goes offline)
    pub fn remove(&mut self, stone_id: &str) -> bool {
        if self.beacons.remove(stone_id).is_some() {
            tracing::debug!(stone_id = %stone_id, "Storage cache entry removed");
            true
        } else {
            false
        }
    }

    /// Get all stones with S3 gateway capability
    pub fn find_s3_gateways(&self) -> Vec<(&str, &SeedBankAnnouncement)> {
        let mut results = Vec::new();
        for (stone_id, beacon) in &self.beacons {
            for sb in &beacon.seed_banks {
                if sb.protocols.contains(&"s3".to_string()) {
                    results.push((stone_id.as_str(), sb));
                }
            }
        }
        results
    }

    /// Find a specific seed bank by name across all stones
    pub fn find_by_name(&self, name: &str) -> Option<(&str, &SeedBankAnnouncement)> {
        for (stone_id, beacon) in &self.beacons {
            for sb in &beacon.seed_banks {
                if sb.name == name {
                    return Some((stone_id.as_str(), sb));
                }
            }
        }
        None
    }

    /// Find a seed bank by ID across all stones
    pub fn find_by_id(&self, id: &str) -> Option<(&str, &SeedBankAnnouncement)> {
        for (stone_id, beacon) in &self.beacons {
            for sb in &beacon.seed_banks {
                if sb.id == id {
                    return Some((stone_id.as_str(), sb));
                }
            }
        }
        None
    }

    /// Get all beacons
    pub fn all_beacons(&self) -> Vec<&StorageBeacon> {
        self.beacons.values().collect()
    }

    /// Get beacon for a specific stone
    pub fn get_beacon(&self, stone_id: &str) -> Option<&StorageBeacon> {
        self.beacons.get(stone_id)
    }

    /// Get endpoint for a stone with storage
    pub fn get_endpoint(&self, stone_id: &str) -> Option<&str> {
        self.beacons.get(stone_id).map(|b| b.endpoint.as_str())
    }

    /// Count stones with storage
    pub fn count_stones(&self) -> usize {
        self.beacons.len()
    }

    /// Count total seed banks across all stones
    pub fn count_seed_banks(&self) -> usize {
        self.beacons.values().map(|b| b.seed_banks.len()).sum()
    }

    /// Check if a stone_id exists in topology (for cache validity)
    pub async fn is_valid(&self, stone_id: &str, topology: &TopologyCache) -> bool {
        let map = topology.read().await;
        map.contains_key(stone_id)
    }

    /// Prune entries for stones no longer in topology
    pub async fn prune_stale(&mut self, topology: &TopologyCache) -> usize {
        let map = topology.read().await;
        let stale_ids: Vec<String> = self
            .beacons
            .keys()
            .filter(|id| !map.contains_key(*id))
            .cloned()
            .collect();
        drop(map);

        let count = stale_ids.len();
        for id in stale_ids {
            self.beacons.remove(&id);
            tracing::debug!(stone_id = %id, "Pruned stale storage cache entry");
        }
        count
    }
}

/// Update storage cache from a beacon
pub async fn update_from_beacon(cache: &StorageCache, beacon: StorageBeacon) {
    let mut inner = cache.write().await;
    inner.update(beacon);
}

/// Remove stone from storage cache
pub async fn remove_stone(cache: &StorageCache, stone_id: &str) -> bool {
    let mut inner = cache.write().await;
    inner.remove(stone_id)
}

/// Find S3 gateways in cache
pub async fn find_s3_gateways(cache: &StorageCache) -> Vec<(String, SeedBankAnnouncement)> {
    let inner = cache.read().await;
    inner
        .find_s3_gateways()
        .into_iter()
        .map(|(id, sb)| (id.to_string(), sb.clone()))
        .collect()
}

/// Find seed bank by name
pub async fn find_by_name(
    cache: &StorageCache,
    name: &str,
) -> Option<(String, SeedBankAnnouncement)> {
    let inner = cache.read().await;
    inner
        .find_by_name(name)
        .map(|(id, sb)| (id.to_string(), sb.clone()))
}

/// Prune stale entries
pub async fn prune_stale(cache: &StorageCache, topology: &TopologyCache) -> usize {
    let mut inner = cache.write().await;
    inner.prune_stale(topology).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use garden_common::storage::StorageAccess;

    fn make_test_beacon(stone_id: &str, stone_name: &str, seed_banks: Vec<&str>) -> StorageBeacon {
        StorageBeacon {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            endpoint: format!("http://{}.local:7185", stone_name),
            seed_banks: seed_banks
                .into_iter()
                .map(|name| SeedBankAnnouncement {
                    id: format!("sb-{}", name),
                    name: name.to_string(),
                    protocols: vec!["s3".to_string(), "storage".to_string()],
                    access: StorageAccess::Direct,
                    visibility: "open".to_string(),
                    health: "healthy".to_string(),
                    capacity_bytes: 1_000_000_000,
                    used_bytes: 500_000_000,
                })
                .collect(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_update_and_find() {
        let mut cache = StorageCacheInner::default();

        let beacon1 = make_test_beacon("stone-1", "alpha", vec!["backup", "media"]);
        let beacon2 = make_test_beacon("stone-2", "beta", vec!["archive"]);

        cache.update(beacon1);
        cache.update(beacon2);

        assert_eq!(cache.count_stones(), 2);
        assert_eq!(cache.count_seed_banks(), 3);

        // Find by name
        let found = cache.find_by_name("backup");
        assert!(found.is_some());
        let (stone_id, sb) = found.unwrap();
        assert_eq!(stone_id, "stone-1");
        assert_eq!(sb.name, "backup");

        // Find S3 gateways
        let gateways = cache.find_s3_gateways();
        assert_eq!(gateways.len(), 3);
    }

    #[test]
    fn test_remove() {
        let mut cache = StorageCacheInner::default();

        let beacon = make_test_beacon("stone-1", "alpha", vec!["backup"]);
        cache.update(beacon);

        assert_eq!(cache.count_stones(), 1);
        assert!(cache.remove("stone-1"));
        assert_eq!(cache.count_stones(), 0);
        assert!(!cache.remove("stone-1")); // Already removed
    }

    #[test]
    fn test_empty_beacon() {
        let mut cache = StorageCacheInner::default();

        // Stone with no seed banks
        let beacon = StorageBeacon::empty("stone-1", "alpha", "http://alpha.local:7185");
        cache.update(beacon);

        assert_eq!(cache.count_stones(), 1);
        assert_eq!(cache.count_seed_banks(), 0);
        assert!(cache.find_s3_gateways().is_empty());
    }
}
