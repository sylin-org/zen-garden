//! Debouncer - Rate limiting for events by composite key
//!
//! Provides a generic debouncer that rate-limits events based on a composite key.
//! Events with the same key within the debounce window are coalesced.
//!
//! # Usage
//!
//! ```ignore
//! use garden_common::infra::Debouncer;
//! use std::time::Duration;
//!
//! let debouncer = Debouncer::new(Duration::from_millis(500));
//!
//! // First event for this key passes
//! assert!(debouncer.should_pass(&("deploy", "mongodb")));
//!
//! // Same key within window is debounced
//! assert!(!debouncer.should_pass(&("deploy", "mongodb")));
//!
//! // Different key passes
//! assert!(debouncer.should_pass(&("deploy", "redis")));
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Default debounce duration (500ms)
pub const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// A generic debouncer that rate-limits events by key
///
/// Events with the same key within the debounce window are coalesced.
/// The key can be any hashable type - commonly a tuple like `(event_type, offering_id)`.
pub struct Debouncer<K: Hash + Eq + Clone> {
    /// Debounce window duration
    duration: Duration,
    /// Last event time per key
    state: RwLock<DebouncerState<K>>,
}

struct DebouncerState<K: Hash + Eq + Clone> {
    /// Last event time per key
    events: HashMap<K, Instant>,
    /// Last cleanup time
    last_cleanup: Instant,
}

impl<K: Hash + Eq + Clone> Default for DebouncerState<K> {
    fn default() -> Self {
        Self {
            events: HashMap::new(),
            last_cleanup: Instant::now(),
        }
    }
}

impl<K: Hash + Eq + Clone> Debouncer<K> {
    /// Create a new debouncer with the given window duration
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            state: RwLock::new(DebouncerState::default()),
        }
    }

    /// Create with default duration (500ms)
    pub fn default_duration() -> Self {
        Self::new(Duration::from_millis(DEFAULT_DEBOUNCE_MS))
    }

    /// Check if an event with this key should pass (not debounced)
    ///
    /// Returns `true` if the event should pass, `false` if it should be debounced.
    /// Automatically records the event if it passes.
    pub fn should_pass(&self, key: &K) -> bool {
        let mut state = self.state.write().unwrap();
        let now = Instant::now();

        // Check if this key recently triggered
        if let Some(last_time) = state.events.get(key)
            && now.duration_since(*last_time) < self.duration
        {
            return false;
        }

        // Record this event
        state.events.insert(key.clone(), now);

        // Periodic cleanup (every 10 seconds)
        if now.duration_since(state.last_cleanup) > Duration::from_secs(10) {
            let cutoff = now - self.duration * 2;
            state.events.retain(|_, v| *v > cutoff);
            state.last_cleanup = now;
        }

        true
    }

    /// Check without recording (peek)
    ///
    /// Returns `true` if an event with this key would pass right now.
    /// Does NOT record the event.
    pub fn would_pass(&self, key: &K) -> bool {
        let state = self.state.read().unwrap();
        let now = Instant::now();

        if let Some(last_time) = state.events.get(key) {
            now.duration_since(*last_time) >= self.duration
        } else {
            true
        }
    }

    /// Reset the debouncer, clearing all recorded events
    pub fn reset(&self) {
        let mut state = self.state.write().unwrap();
        state.events.clear();
        state.last_cleanup = Instant::now();
    }

    /// Get the number of tracked keys
    pub fn tracked_count(&self) -> usize {
        self.state.read().unwrap().events.len()
    }

    /// Get the debounce duration
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

/// Convenience type alias for string key pairs (e.g., event_type + offering_id)
pub type StringPairDebouncer = Debouncer<(String, String)>;

impl StringPairDebouncer {
    /// Check if an event should pass using string slice keys
    ///
    /// Convenience method to avoid allocations when keys are known to be unique.
    pub fn should_pass_str(&self, key1: &str, key2: &str) -> bool {
        self.should_pass(&(key1.to_string(), key2.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_first_event_passes() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));
        assert!(debouncer.should_pass(&"test".to_string()));
    }

    #[test]
    fn test_same_key_debounced() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));

        assert!(debouncer.should_pass(&"test".to_string()));
        assert!(!debouncer.should_pass(&"test".to_string()));
        assert!(!debouncer.should_pass(&"test".to_string()));
    }

    #[test]
    fn test_different_keys_pass() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));

        assert!(debouncer.should_pass(&"key1".to_string()));
        assert!(debouncer.should_pass(&"key2".to_string()));
        assert!(debouncer.should_pass(&"key3".to_string()));
    }

    #[test]
    fn test_window_expires() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(50));

        assert!(debouncer.should_pass(&"test".to_string()));
        assert!(!debouncer.should_pass(&"test".to_string()));

        // Wait for window to expire
        sleep(Duration::from_millis(100));

        assert!(debouncer.should_pass(&"test".to_string()));
    }

    #[test]
    fn test_tuple_key() {
        let debouncer: Debouncer<(String, String)> = Debouncer::new(Duration::from_millis(100));

        // Same type, same offering = debounced
        assert!(debouncer.should_pass(&("deploy".to_string(), "mongodb".to_string())));
        assert!(!debouncer.should_pass(&("deploy".to_string(), "mongodb".to_string())));

        // Different type, same offering = passes
        assert!(debouncer.should_pass(&("update".to_string(), "mongodb".to_string())));

        // Same type, different offering = passes
        assert!(debouncer.should_pass(&("deploy".to_string(), "redis".to_string())));
    }

    #[test]
    fn test_string_pair_debouncer() {
        let debouncer = StringPairDebouncer::new(Duration::from_millis(100));

        assert!(debouncer.should_pass_str("event", "offering"));
        assert!(!debouncer.should_pass_str("event", "offering"));
        assert!(debouncer.should_pass_str("event", "other"));
        assert!(debouncer.should_pass_str("other", "offering"));
    }

    #[test]
    fn test_would_pass_no_record() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));

        // would_pass doesn't record
        assert!(debouncer.would_pass(&"test".to_string()));
        assert!(debouncer.would_pass(&"test".to_string()));

        // After should_pass, would_pass returns false
        assert!(debouncer.should_pass(&"test".to_string()));
        assert!(!debouncer.would_pass(&"test".to_string()));
    }

    #[test]
    fn test_reset() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));

        debouncer.should_pass(&"test".to_string());
        assert!(!debouncer.should_pass(&"test".to_string()));

        debouncer.reset();

        // After reset, key passes again
        assert!(debouncer.should_pass(&"test".to_string()));
    }

    #[test]
    fn test_tracked_count() {
        let debouncer: Debouncer<String> = Debouncer::new(Duration::from_millis(100));

        assert_eq!(debouncer.tracked_count(), 0);

        debouncer.should_pass(&"key1".to_string());
        assert_eq!(debouncer.tracked_count(), 1);

        debouncer.should_pass(&"key2".to_string());
        assert_eq!(debouncer.tracked_count(), 2);

        // Duplicate doesn't increase count
        debouncer.should_pass(&"key1".to_string());
        assert_eq!(debouncer.tracked_count(), 2);
    }
}
