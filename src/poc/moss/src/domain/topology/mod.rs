//! Topology management and stone discovery
//!
//! In-memory cache of discovered stones for placement, service discovery,
//! and Wake-on-LAN support.
//!
//! ## Design: Offline Marking vs TTL Eviction
//!
//! Instead of evicting stones after a short TTL, stones are marked as Offline
//! when not seen for 90 seconds. This preserves MAC addresses for Wake-on-LAN
//! even after stones shut down.
//!
//! - Max 64 offline stones tracked (LRU eviction when cap reached)
//! - Offline stones evicted after 24 hours
//!
//! ## Persistence (TOPO-0002)
//!
//! The cache is persisted to `{topology_dir}/garden-topology.json` as a bare
//! JSON array of TopologyEntry objects (self entry first, then peers).
//!
//! Write triggers:
//! - Dirty flag set on cache mutation (upsert, mark offline, forget)
//! - Periodic flush every 30s during maintenance (if dirty)
//! - Graceful shutdown flush (immediate)
//!
//! Uses atomic write (tmp + rename) via `garden_common::persistence::atomic_write_file`.

pub mod aggregate;
pub mod composition;
pub mod error;
pub mod event;
pub mod store;
pub mod transport;

pub use aggregate::{SelfEntryInputs, Topology};
pub use error::TopologyError;
pub use event::{ChangeKind, TopologyChanged};
pub use store::{FileTopologyStore, TopologyStore};
pub use transport::{ChirpTransport, NoopChirpTransport, P2pChirpTransport};

use chrono::{Duration, Utc};
use garden_common::{StoneStatus, TopologyEntry};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

/// Maximum number of offline stones to track
const MAX_OFFLINE_STONES: usize = 64;

/// Threshold for marking a stone as offline (seconds since last seen)
///
/// Stones chirp every 30s. At 90s (3 chirp cycles) we tolerate 2 missed
/// chirps before declaring a stone offline — enough headroom for normal
/// UDP jitter on busy LANs. Graceful shutdowns are handled immediately
/// via STONE_GOODBYE, so this threshold only governs crash/network-loss
/// detection.
const OFFLINE_THRESHOLD_SECS: i64 = 90;

/// TTL for offline stones before eviction (hours)
const OFFLINE_EVICTION_HOURS: i64 = 24;

/// In-memory topology cache.
///
/// Stores all discovered stones indexed by stone_id. Populated from
/// UDP discovery responses and mDNS announcements. Private to the
/// topology module — the `Topology` aggregate is the only owner.
pub(crate) type TopologyCache = Arc<RwLock<HashMap<String, TopologyEntry>>>;

/// Shared dirty flag for topology persistence (private).
pub(crate) type TopologyDirtyFlag = Arc<AtomicBool>;

/// Mark the topology cache as dirty (needs persistence)
fn mark_dirty(dirty: &TopologyDirtyFlag) {
    dirty.store(true, Ordering::Relaxed);
}

/// Add or update a stone from a chirp (received TopologyEntry)
///
/// Primary method for updating topology cache from stone chirps.
/// The chirp IS a TopologyEntry being broadcast.
pub(super) async fn upsert_from_chirp(cache: &TopologyCache, mut chirped_entry: TopologyEntry) {
    let mut map = cache.write().await;
    let now = Utc::now();

    if let Some(entry) = map.get_mut(&chirped_entry.stone_id) {
        // Update existing entry - stone is back online
        entry.stone_name = chirped_entry.stone_name;
        entry.address = chirped_entry.address.clone();
        entry.moss_version = chirped_entry.moss_version;
        entry.services = chirped_entry.services;
        entry.gateways = chirped_entry.gateways;
        entry.tags = chirped_entry.tags;
        entry.health = chirped_entry.health;
        entry.capabilities = chirped_entry.capabilities;
        // Only update MAC if provided (preserve existing if None)
        if chirped_entry.mac.is_some() {
            entry.mac = chirped_entry.mac;
        }
        entry.status = StoneStatus::Online;
        entry.last_seen = now;
    } else {
        // Insert new entry with current timestamp
        chirped_entry.status = StoneStatus::Online;
        chirped_entry.discovered_at = now;
        chirped_entry.last_seen = now;
        map.insert(chirped_entry.stone_id.clone(), chirped_entry);
    }
}

/// Add or update a stone from a chirp, and mark dirty flag for persistence
pub(super) async fn upsert_from_chirp_dirty(
    cache: &TopologyCache,
    chirped_entry: TopologyEntry,
    dirty: &TopologyDirtyFlag,
) {
    upsert_from_chirp(cache, chirped_entry).await;
    mark_dirty(dirty);
}

/// Get all stones from topology cache (both online and offline)
pub(super) async fn get_all_stones(cache: &TopologyCache) -> Vec<TopologyEntry> {
    let map = cache.read().await;
    map.values().cloned().collect()
}

/// Get only online stones from topology cache
pub(super) async fn get_online_stones(cache: &TopologyCache) -> Vec<TopologyEntry> {
    let map = cache.read().await;
    map.values()
        .filter(|e| e.status == StoneStatus::Online)
        .cloned()
        .collect()
}

/// Get a specific stone by ID (regardless of status)
pub(super) async fn get_stone_by_id(
    cache: &TopologyCache,
    stone_id: &str,
) -> Option<TopologyEntry> {
    let map = cache.read().await;
    map.get(stone_id).cloned()
}

/// Get a specific stone by name (regardless of status)
pub(super) async fn get_stone_by_name(
    cache: &TopologyCache,
    stone_name: &str,
) -> Option<TopologyEntry> {
    let map = cache.read().await;
    map.values()
        .find(|entry| entry.stone_name == stone_name)
        .cloned()
}

/// Count stones in topology cache
pub(super) async fn count_stones(cache: &TopologyCache) -> usize {
    let map = cache.read().await;
    map.len()
}

/// Count online stones in topology cache
pub(super) async fn count_online_stones(cache: &TopologyCache) -> usize {
    let map = cache.read().await;
    map.values()
        .filter(|e| e.status == StoneStatus::Online)
        .count()
}

/// Mark stale stones as offline and evict very old offline stones
///
/// This replaces the old `prune_stale_stones` function with offline-marking semantics:
/// 1. Stones not seen for 90s → marked Offline (but retained)
/// 2. Offline stones older than 24h → evicted
/// 3. If more than MAX_OFFLINE_STONES offline → evict oldest (LRU)
///
/// Returns (marked_offline_count, evicted_count)
pub(super) async fn maintain_topology(cache: &TopologyCache) -> (usize, usize) {
    let mut map = cache.write().await;
    let now = Utc::now();
    let offline_threshold = Duration::seconds(OFFLINE_THRESHOLD_SECS);
    let eviction_threshold = Duration::hours(OFFLINE_EVICTION_HOURS);

    let mut marked_offline = 0;
    let mut evicted;

    // Phase 1: Mark stale online stones as offline
    for entry in map.values_mut() {
        if entry.status == StoneStatus::Online {
            let age = now.signed_duration_since(entry.last_seen);
            if age > offline_threshold {
                entry.status = StoneStatus::Offline;
                entry.health = garden_common::constants::VITALITY_DORMANT.to_string();
                marked_offline += 1;
                tracing::debug!(
                    stone_name = %entry.stone_name,
                    last_seen = %entry.last_seen,
                    "Stone marked offline (stale)"
                );
            }
        }
    }

    // Phase 2: Evict offline stones older than 24h
    let initial_count = map.len();
    map.retain(|_, entry| {
        if entry.status == StoneStatus::Offline {
            let age = now.signed_duration_since(entry.last_seen);
            age <= eviction_threshold
        } else {
            true
        }
    });
    evicted = initial_count - map.len();

    // Phase 3: Enforce max offline stone cap (LRU eviction)
    let offline_count = map
        .values()
        .filter(|e| e.status == StoneStatus::Offline)
        .count();
    if offline_count > MAX_OFFLINE_STONES {
        let excess = offline_count - MAX_OFFLINE_STONES;

        // Collect offline stones sorted by last_seen (oldest first)
        let mut offline_stones: Vec<_> = map
            .iter()
            .filter(|(_, e)| e.status == StoneStatus::Offline)
            .map(|(id, e)| (id.clone(), e.last_seen))
            .collect();
        offline_stones.sort_by_key(|(_, last_seen)| *last_seen);

        // Remove the oldest ones
        for (stone_id, _) in offline_stones.into_iter().take(excess) {
            if let Some(entry) = map.remove(&stone_id) {
                tracing::debug!(
                    stone_name = %entry.stone_name,
                    "Evicted offline stone (LRU cap reached)"
                );
                evicted += 1;
            }
        }
    }

    (marked_offline, evicted)
}

/// Maintain topology and flush to disk if dirty (TOPO-0002)
///
/// Called every 30s by the topology maintenance task.
/// Combines marking/eviction with persistence.
pub(super) async fn maintain_and_persist(
    cache: &TopologyCache,
    dirty: &TopologyDirtyFlag,
    self_entry: &TopologyEntry,
) -> (usize, usize) {
    let (marked, evicted) = maintain_topology(cache).await;

    // Maintenance itself can dirty the cache (offline marking, eviction)
    if marked > 0 || evicted > 0 {
        mark_dirty(dirty);
    }

    // Flush to disk if dirty
    if dirty.swap(false, Ordering::Relaxed)
        && let Err(e) = persist_topology(cache, self_entry).await
    {
        tracing::warn!(error = %e, "Failed to persist topology to disk");
        // Re-dirty so next cycle retries
        mark_dirty(dirty);
    }

    (marked, evicted)
}

/// Mark a stone as offline by stone_id (graceful goodbye)
///
/// Called when receiving a STONE_GOODBYE announcement.
/// Returns true if the stone was found and marked offline.
pub(super) async fn mark_stone_offline(cache: &TopologyCache, stone_id: &str) -> bool {
    let mut map = cache.write().await;
    if let Some(entry) = map.get_mut(stone_id)
        && entry.status != StoneStatus::Offline
    {
        entry.status = StoneStatus::Offline;
        // Set health to dormant so UI (observe) renders consistently.
        // The last-known health is no longer meaningful once the stone
        // stops responding — "dormant" is the garden metaphor for sleeping.
        entry.health = garden_common::constants::VITALITY_DORMANT.to_string();
        tracing::info!(
            stone_name = %entry.stone_name,
            "Stone marked offline (goodbye received)"
        );
        return true;
    }
    false
}

/// Mark a stone as offline and set dirty flag for persistence
pub(super) async fn mark_stone_offline_dirty(
    cache: &TopologyCache,
    stone_id: &str,
    dirty: &TopologyDirtyFlag,
) -> bool {
    let result = mark_stone_offline(cache, stone_id).await;
    if result {
        mark_dirty(dirty);
    }
    result
}

/// Remove a specific stone from the cache (explicit forget)
pub(super) async fn forget_stone(cache: &TopologyCache, stone_name: &str) -> bool {
    let mut map = cache.write().await;
    let stone_id = map
        .values()
        .find(|e| e.stone_name == stone_name)
        .map(|e| e.stone_id.clone());

    if let Some(id) = stone_id {
        map.remove(&id).is_some()
    } else {
        false
    }
}

/// Remove a specific stone and set dirty flag for persistence
pub(super) async fn forget_stone_dirty(
    cache: &TopologyCache,
    stone_name: &str,
    dirty: &TopologyDirtyFlag,
) -> bool {
    let result = forget_stone(cache, stone_name).await;
    if result {
        mark_dirty(dirty);
    }
    result
}

// ============================================================================
// Persistence (TOPO-0002)
// ============================================================================

/// Persist the topology cache to disk as a bare JSON array
///
/// Writes self entry first, then all cached peers (skipping self).
/// Format: bare `TopologyEntry[]` (not the API envelope).
/// Uses atomic write (tmp + rename) for crash safety.
pub(super) async fn persist_topology(
    cache: &TopologyCache,
    self_entry: &TopologyEntry,
) -> Result<(), anyhow::Error> {
    let self_id = self_entry.stone_id.clone();

    // Build array: self first, then peers
    let mut stones = vec![self_entry.clone()];
    let cache_entries = {
        let map = cache.read().await;
        map.values().cloned().collect::<Vec<_>>()
    };
    for entry in cache_entries {
        if entry.stone_id == self_id {
            continue;
        }
        stones.push(entry);
    }

    let json = serde_json::to_string_pretty(&stones)
        .map_err(|e| anyhow::anyhow!("Failed to serialize topology: {}", e))?;

    let path = std::path::PathBuf::from(garden_common::constants::paths::topology_dir())
        .join(garden_common::constants::paths::TOPOLOGY_FILE);

    garden_common::persistence::atomic_write_file(&path, json.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write topology file {}: {}", path.display(), e))?;

    tracing::debug!(
        stones = stones.len(),
        path = %path.display(),
        "Topology persisted to disk"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_cache() -> TopologyCache {
        Arc::new(RwLock::new(HashMap::new()))
    }

    /// Helper to create a minimal TopologyEntry for testing
    fn make_entry(
        stone_id: &str,
        stone_name: &str,
        endpoint: &str,
        version: &str,
    ) -> TopologyEntry {
        // Parse endpoint string like "http://192.168.1.10:7123" → PeerAddress
        let addr = parse_endpoint_to_peer_address(endpoint);
        TopologyEntry {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            address: addr,
            moss_version: version.to_string(),
            services: vec![],
            mac: None,
            health: "thriving".to_string(),
            capabilities: None,
            status: StoneStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            tags: vec![],
            gateways: vec![],
        }
    }

    /// Parse a test endpoint string like "http://192.168.1.10:7123" into a PeerAddress
    fn parse_endpoint_to_peer_address(endpoint: &str) -> garden_common::PeerAddress {
        let stripped = endpoint
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let (ip_str, port_str) = stripped.rsplit_once(':').unwrap_or((stripped, "7185"));
        garden_common::PeerAddress::new(
            ip_str
                .parse()
                .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            port_str.parse().unwrap_or(7185),
        )
    }

    /// Helper to create a TopologyEntry with MAC for testing
    fn make_entry_with_mac(
        stone_id: &str,
        stone_name: &str,
        endpoint: &str,
        version: &str,
        mac: Option<&str>,
    ) -> TopologyEntry {
        let addr = parse_endpoint_to_peer_address(endpoint);
        TopologyEntry {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            address: addr,
            moss_version: version.to_string(),
            services: vec![],
            mac: mac.map(|s| s.to_string()),
            health: "thriving".to_string(),
            capabilities: None,
            status: StoneStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            tags: vec![],
            gateways: vec![],
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let cache = make_test_cache();

        let entry = make_entry("stone-123", "oak", "http://192.168.1.10:7123", "0.1.0");
        upsert_from_chirp(&cache, entry).await;

        let stone = get_stone_by_id(&cache, "stone-123").await;
        assert!(stone.is_some());
        let stone = stone.unwrap();
        assert_eq!(stone.stone_name, "oak");
        assert_eq!(stone.status, StoneStatus::Online);

        assert_eq!(count_stones(&cache).await, 1);
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let cache = make_test_cache();

        let entry1 = make_entry("stone-123", "oak", "http://192.168.1.10:7123", "0.1.0");
        upsert_from_chirp(&cache, entry1).await;

        // Update with new endpoint
        let entry2 = make_entry("stone-123", "oak", "http://192.168.1.99:7123", "0.1.1");
        upsert_from_chirp(&cache, entry2).await;

        let stone = get_stone_by_id(&cache, "stone-123").await.unwrap();
        assert_eq!(stone.address.http_base(), "http://192.168.1.99:7123");
        assert_eq!(stone.moss_version, "0.1.1");
        assert_eq!(count_stones(&cache).await, 1); // Still only one entry
    }

    #[tokio::test]
    async fn test_upsert_preserves_mac() {
        let cache = make_test_cache();

        // First upsert with MAC
        let entry1 = make_entry_with_mac(
            "stone-123",
            "oak",
            "http://192.168.1.10:7123",
            "0.1.0",
            Some("AA:BB:CC:DD:EE:FF"),
        );
        upsert_from_chirp(&cache, entry1).await;

        // Update without MAC - should preserve existing
        let entry2 = make_entry_with_mac(
            "stone-123",
            "oak",
            "http://192.168.1.99:7123",
            "0.1.1",
            None,
        );
        upsert_from_chirp(&cache, entry2).await;

        let stone = get_stone_by_id(&cache, "stone-123").await.unwrap();
        assert_eq!(stone.mac, Some("AA:BB:CC:DD:EE:FF".to_string()));
    }

    #[tokio::test]
    async fn test_get_by_name() {
        let cache = make_test_cache();

        let entry1 = make_entry("stone-123", "oak", "http://192.168.1.10:7123", "0.1.0");
        upsert_from_chirp(&cache, entry1).await;

        let entry2 = make_entry("stone-456", "cedar", "http://192.168.1.11:7123", "0.1.0");
        upsert_from_chirp(&cache, entry2).await;

        let stone = get_stone_by_name(&cache, "cedar").await;
        assert!(stone.is_some());
        assert_eq!(stone.unwrap().stone_id, "stone-456");
    }

    #[tokio::test]
    async fn test_get_all_stones() {
        let cache = make_test_cache();

        upsert_from_chirp(
            &cache,
            make_entry("s1", "oak", "http://10.0.0.1:7123", "0.1.0"),
        )
        .await;
        upsert_from_chirp(
            &cache,
            make_entry("s2", "cedar", "http://10.0.0.2:7123", "0.1.0"),
        )
        .await;
        upsert_from_chirp(
            &cache,
            make_entry("s3", "maple", "http://10.0.0.3:7123", "0.1.0"),
        )
        .await;

        let all = get_all_stones(&cache).await;
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_stone_status_display() {
        assert_eq!(format!("{}", StoneStatus::Online), "online");
        assert_eq!(format!("{}", StoneStatus::Offline), "offline");
    }

    #[tokio::test]
    async fn test_forget_stone() {
        let cache = make_test_cache();

        upsert_from_chirp(
            &cache,
            make_entry("s1", "oak", "http://10.0.0.1:7123", "0.1.0"),
        )
        .await;
        upsert_from_chirp(
            &cache,
            make_entry("s2", "cedar", "http://10.0.0.2:7123", "0.1.0"),
        )
        .await;

        assert_eq!(count_stones(&cache).await, 2);

        let removed = forget_stone(&cache, "oak").await;
        assert!(removed);
        assert_eq!(count_stones(&cache).await, 1);

        let removed_again = forget_stone(&cache, "oak").await;
        assert!(!removed_again);
    }

    #[tokio::test]
    async fn test_dirty_flag_basics() {
        let dirty: TopologyDirtyFlag = Arc::new(AtomicBool::new(true));
        // Starts dirty — first maintenance cycle writes initial file
        assert!(dirty.load(Ordering::Relaxed));

        // swap clears it
        let was_dirty = dirty.swap(false, Ordering::Relaxed);
        assert!(was_dirty);
        assert!(!dirty.load(Ordering::Relaxed));

        // mark_dirty sets it again
        mark_dirty(&dirty);
        assert!(dirty.load(Ordering::Relaxed));

        // swap clears again
        let was_dirty = dirty.swap(false, Ordering::Relaxed);
        assert!(was_dirty);
        assert!(!dirty.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_upsert_dirty_sets_flag() {
        let cache = make_test_cache();
        let dirty: TopologyDirtyFlag = Arc::new(AtomicBool::new(true));

        let entry = make_entry("s1", "oak", "http://10.0.0.1:7123", "0.1.0");
        upsert_from_chirp_dirty(&cache, entry, &dirty).await;

        assert!(dirty.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_persist_topology_writes_file() {
        let cache = make_test_cache();
        let self_entry = make_entry("self-1", "local", "http://127.0.0.1:7185", "0.1.0");

        // Add a peer
        upsert_from_chirp(
            &cache,
            make_entry("peer-1", "oak", "http://10.0.0.1:7185", "0.1.0"),
        )
        .await;

        // Write to temp directory
        let temp_dir = std::env::temp_dir().join("zen-garden-test-topology");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        // Override shared_data_dir so topology_dir resolves to our temp dir
        let original = std::env::var("GARDEN_SHARED_DATA_DIR").ok();
        // SAFETY: Test-only; no concurrent env var access in this test.
        unsafe { std::env::set_var("GARDEN_SHARED_DATA_DIR", temp_dir.to_str().unwrap()) };

        let result = persist_topology(&cache, &self_entry).await;
        assert!(result.is_ok());

        // Read back and verify
        let file_path = temp_dir.join("topology").join("garden-topology.json");
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        let entries: Vec<TopologyEntry> = serde_json::from_str(&content).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].stone_id, "self-1"); // Self first
        assert_eq!(entries[1].stone_id, "peer-1"); // Then peer

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        match original {
            // SAFETY: Test cleanup — restoring original env var.
            Some(val) => unsafe { std::env::set_var("GARDEN_SHARED_DATA_DIR", val) },
            None => unsafe { std::env::remove_var("GARDEN_SHARED_DATA_DIR") },
        }
    }
}
