//! Client-side stone discovery cache with TTL
//!
//! Hot cache architecture for stone discovery results.
//! Used across commands to avoid redundant network requests.
//!
//! # Features
//! - 90-second TTL for balancing freshness with performance
//! - Global singleton via `GLOBAL_CACHE`
//! - Thread-safe via `Arc<Mutex>`
//! - Automatic expiration
//!
//! # Example
//! ```ignore
//! use garden_common::client::stone_cache::{GLOBAL_CACHE, CachedStone};
//!
//! // Check cache first
//! if let Some(cached) = GLOBAL_CACHE.get("stone-02") {
//!     println!("Found: {}", cached.endpoint);
//! } else {
//!     // Discovery and insert
//!     // GLOBAL_CACHE.insert(endpoint, capabilities);
//! }
//! ```

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(90);

/// Global stone cache singleton
///
/// Single source of truth for stone discovery caching.
pub static GLOBAL_CACHE: Lazy<StoneCache> = Lazy::new(StoneCache::new);

#[derive(Clone, Debug)]
pub struct CachedStone {
    pub endpoint: String,
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub last_seen: Instant,
}

impl CachedStone {
    /// Get the cache key for this stone (stone_id if available, otherwise stone_name)
    pub fn cache_key(&self) -> String {
        self.stone_id.clone().unwrap_or_else(|| self.stone_name.clone())
    }
}

pub struct StoneCache {
    stones: Arc<Mutex<HashMap<String, CachedStone>>>,
}

impl StoneCache {
    pub fn new() -> Self {
        Self {
            stones: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get stone from cache if not expired
    pub fn get(&self, stone_name: &str) -> Option<CachedStone> {
        let mut cache = self.stones.lock().unwrap();
        
        if let Some(cached) = cache.get(stone_name) {
            // Check if still valid (TTL not expired)
            if cached.last_seen.elapsed() < CACHE_TTL {
                tracing::info!(
                    stone = %stone_name,
                    age_secs = %cached.last_seen.elapsed().as_secs(),
                    "Cache hit - returning cached stone"
                );
                return Some(cached.clone());
            } else {
                // Expired, remove from cache
                tracing::info!(stone = %stone_name, "Cache entry expired (TTL 90s)");
                cache.remove(stone_name);
            }
        }
        
        tracing::debug!(stone = %stone_name, "Cache miss");
        None
    }

    /// Insert a stone into the cache
    ///
    /// Uses stone_id as the cache key when available, falling back to stone_name.
    /// This ensures stable caching even when hostname changes.
    pub fn insert(&self, endpoint: String, stone_id: Option<String>, stone_name: String) {
        let mut cache = self.stones.lock().unwrap();

        // Use stone_id as key when available, otherwise use stone_name
        let cache_key = stone_id.clone().unwrap_or_else(|| stone_name.clone());

        cache.insert(
            cache_key.clone(),
            CachedStone {
                endpoint,
                stone_id,
                stone_name: stone_name.clone(),
                last_seen: Instant::now(),
            },
        );
        tracing::debug!(stone = %stone_name, key = %cache_key, "Cached stone discovery");
    }

    /// Get all stones from cache (removes expired entries)
    pub fn get_all(&self) -> Vec<CachedStone> {
        let mut cache = self.stones.lock().unwrap();
        
        // Remove expired entries
        cache.retain(|stone_name, cached| {
            let valid = cached.last_seen.elapsed() < CACHE_TTL;
            if !valid {
                tracing::debug!(stone = %stone_name, "Cache entry expired during get_all");
            }
            valid
        });
        
        cache.values().cloned().collect()
    }

    /// Refresh a stone's TTL
    pub fn refresh_stone(&self, stone_name: &str) -> bool {
        let mut cache = self.stones.lock().unwrap();
        if let Some(cached) = cache.get_mut(stone_name) {
            cached.last_seen = Instant::now();
            tracing::debug!(stone = %stone_name, "Refreshed cache TTL");
            true
        } else {
            false
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        let mut cache = self.stones.lock().unwrap();
        cache.clear();
        tracing::debug!("Cleared stone cache");
    }

    /// Get cache size
    pub fn count(&self) -> usize {
        let cache = self.stones.lock().unwrap();
        cache.len()
    }
}

impl Default for StoneCache {
    fn default() -> Self {
        Self::new()
    }
}
