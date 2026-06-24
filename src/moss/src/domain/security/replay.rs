//! Single-use nonce cache — replay defence for the enforced control plane.
//!
//! koi verifies a signature + a ±300s freshness window but keeps **no** record of
//! seen nonces, so within that window a captured envelope can be replayed and will
//! re-verify identically. For non-idempotent control-plane mutations that is a real
//! replay vector, so zen records each `(signer_cn, nonce)` once and rejects a reuse.
//!
//! The cache only needs to remember one freshness window's worth of nonces: koi
//! independently rejects any envelope whose `ts` is outside ±300s as Stale, so a
//! nonce older than the window can never re-verify and need not be remembered.
//! Eviction is therefore trivial (drop entries whose `ts` left the window), which
//! keeps the set naturally small (bounded by mutation volume × 300s).

use std::collections::HashMap;
use std::sync::Mutex;

/// Matches koi's `FRESHNESS_WINDOW_SECS` — the only window in which a nonce can
/// still re-verify, so the only window we must remember.
const FRESHNESS_WINDOW_SECS: i64 = 300;

/// Backstop bound (code-standards §20): under a flood of distinct nonces the cache
/// fails closed rather than growing without limit. Far above any real homelab
/// mutation rate within a 300s window.
const MAX_ENTRIES: usize = 100_000;

/// Bounded, time-evicted set of seen `(signer_cn, nonce)` pairs.
pub struct NonceCache {
    /// `(signer_cn, nonce)` → the envelope `ts` (signer's unix seconds), used for
    /// window-based eviction.
    seen: Mutex<HashMap<(String, String), i64>>,
}

impl NonceCache {
    pub fn new() -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Record `(cn, nonce)` as used and report whether it was **fresh** (first
    /// use → `true`) or a **replay** (already seen → `false`). Call only after the
    /// envelope has been authenticated — recording the nonce of an unverified
    /// request would let an attacker pre-poison a victim's future nonce.
    ///
    /// `ts` is the envelope's timestamp (used for eviction). Stale entries (whose
    /// `ts` has left the freshness window) are evicted first. Under the
    /// [`MAX_ENTRIES`] backstop the call fails closed (treats the nonce as a
    /// replay) rather than letting the set grow unbounded.
    pub fn check_and_record(&self, cn: &str, nonce: &str, ts: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        let mut seen = self.seen.lock().expect("nonce cache mutex poisoned");

        // Drop nonces that can no longer re-verify (their ts left the window).
        seen.retain(|_, &mut entry_ts| (now - entry_ts).abs() <= FRESHNESS_WINDOW_SECS);

        if seen.len() >= MAX_ENTRIES {
            tracing::warn!(
                entries = seen.len(),
                "Nonce cache at capacity — failing closed (possible replay flood)"
            );
            return false;
        }

        use std::collections::hash_map::Entry;
        match seen.entry((cn.to_string(), nonce.to_string())) {
            // Already used within the window → replay.
            Entry::Occupied(_) => false,
            // First use → record and accept.
            Entry::Vacant(slot) => {
                slot.insert(ts);
                true
            }
        }
    }
}

impl Default for NonceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_use_is_fresh_replay_is_rejected() {
        let cache = NonceCache::new();
        let now = chrono::Utc::now().timestamp();
        assert!(cache.check_and_record("stone-a", "nonce-1", now));
        // Same (cn, nonce) again within the window → replay.
        assert!(!cache.check_and_record("stone-a", "nonce-1", now));
        // Different nonce → fresh.
        assert!(cache.check_and_record("stone-a", "nonce-2", now));
        // Same nonce, different signer → fresh (keyed on both).
        assert!(cache.check_and_record("stone-b", "nonce-1", now));
    }

    #[test]
    fn stale_entries_are_evicted_so_their_nonce_frees() {
        let cache = NonceCache::new();
        let stale_ts = chrono::Utc::now().timestamp() - (FRESHNESS_WINDOW_SECS + 60);
        // A nonce stamped outside the window is recorded but immediately evictable;
        // koi would reject it as Stale anyway, so this path is defensive.
        assert!(cache.check_and_record("stone-a", "old", stale_ts));
        // Next call evicts the stale entry; recording it again is "fresh".
        assert!(cache.check_and_record("stone-a", "old", stale_ts));
    }
}
