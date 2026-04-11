//! Client-side stone discovery with TTL
//!
//! In-process memo for stone discovery results.
//! Used across commands to avoid redundant network requests within a session.
//!
//! # Features
//! - 90-second TTL for balancing freshness with performance
//! - Process-scoped singleton via `STONE`
//! - Thread-safe via `Arc<Mutex>`
//! - Automatic expiration

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(90);

/// Process-scoped stone discovery singleton.
pub static STONE: LazyLock<Discovery> = LazyLock::new(Discovery::new);

/// A recently-discovered stone endpoint.
#[derive(Clone, Debug)]
pub struct KnownStone {
    pub endpoint: String,
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub last_seen: Instant,
}

impl KnownStone {
    /// Get the cache key for this stone (stone_id if available, otherwise stone_name)
    pub fn cache_key(&self) -> String {
        self.stone_id
            .clone()
            .unwrap_or_else(|| self.stone_name.clone())
    }
}

/// In-process registry of known reachable stone endpoints.
pub struct Discovery {
    stone: Arc<Mutex<HashMap<String, KnownStone>>>,
}

impl Discovery {
    pub fn new() -> Self {
        Self {
            stone: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get stone from discovery if not expired
    pub fn get(&self, stone_name: &str) -> Option<KnownStone> {
        let mut stones = self.stone.lock().unwrap();

        if let Some(known) = stones.get(stone_name) {
            if known.last_seen.elapsed() < TTL {
                tracing::info!(
                    stone = %stone_name,
                    age_secs = %known.last_seen.elapsed().as_secs(),
                    "Discovery hit - returning known stone"
                );
                return Some(known.clone());
            } else {
                tracing::info!(stone = %stone_name, "Discovery entry expired (TTL 90s)");
                stones.remove(stone_name);
            }
        }

        tracing::debug!(stone = %stone_name, "Discovery miss");
        None
    }

    /// Insert a stone into discovery
    ///
    /// Uses stone_id as the key when available, falling back to stone_name.
    /// This ensures stable caching even when hostname changes.
    pub fn insert(&self, endpoint: String, stone_id: Option<String>, stone_name: String) {
        let mut stones = self.stone.lock().unwrap();

        let key = stone_id.clone().unwrap_or_else(|| stone_name.clone());

        stones.insert(
            key.clone(),
            KnownStone {
                endpoint,
                stone_id,
                stone_name: stone_name.clone(),
                last_seen: Instant::now(),
            },
        );
        tracing::debug!(stone = %stone_name, key = %key, "Stone added to discovery");
    }

    /// Get all known stones (removes expired entries)
    pub fn get_all(&self) -> Vec<KnownStone> {
        let mut stones = self.stone.lock().unwrap();

        stones.retain(|stone_name, known| {
            let valid = known.last_seen.elapsed() < TTL;
            if !valid {
                tracing::debug!(stone = %stone_name, "Discovery entry expired during get_all");
            }
            valid
        });

        stones.values().cloned().collect()
    }

    /// Refresh a stone's TTL
    pub fn refresh(&self, stone_name: &str) -> bool {
        let mut stones = self.stone.lock().unwrap();
        if let Some(known) = stones.get_mut(stone_name) {
            known.last_seen = Instant::now();
            tracing::debug!(stone = %stone_name, "Refreshed discovery TTL");
            true
        } else {
            false
        }
    }

    /// Clear all entries
    pub fn clear(&self) {
        let mut stones = self.stone.lock().unwrap();
        stones.clear();
        tracing::debug!("Cleared stone discovery");
    }

    /// Entry count
    pub fn count(&self) -> usize {
        let stones = self.stone.lock().unwrap();
        stones.len()
    }
}

impl Default for Discovery {
    fn default() -> Self {
        Self::new()
    }
}
