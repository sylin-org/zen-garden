//! Notification Registry for Stone Tags
//!
//! Provides a decoupled notification system where subsystems can register
//! their state (opportunity/attention) and the registry compiles these into
//! tags for topology chirps.
//!
//! ## Design
//!
//! - Each subsystem registers with a source key and tag
//! - Multiple subsystems can contribute the same tag type
//! - When a subsystem's condition clears, it removes its entry
//! - The registry compiles unique tags for chirping
//!
//! ## Example
//!
//! ```ignore
//! // Subsystem detects candidates
//! registry.set(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity);
//!
//! // Later, candidates are cleared
//! registry.clear(NOTIF_SOURCE_CANDIDATES);
//!
//! // Compile for chirp
//! let tags = registry.compile(); // ["opportunity"] or []
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// Notification Tags
// ============================================================================

/// Notification tag types
///
/// These represent the semantic state that gets chirped to other stones.
/// Keep this enum small - it's the public contract for topology chirps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationTag {
    /// Something hopeful waiting (candidate devices, available updates)
    Opportunity,
    /// Something needs attention (degraded health, errors)
    Attention,
}

impl NotificationTag {
    /// Convert to wire format string
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationTag::Opportunity => TAG_OPPORTUNITY,
            NotificationTag::Attention => TAG_ATTENTION,
        }
    }
}

impl std::fmt::Display for NotificationTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Tag Wire Format Constants
// ============================================================================

/// Wire format for opportunity tag
pub const TAG_OPPORTUNITY: &str = "opportunity";

/// Wire format for attention tag
pub const TAG_ATTENTION: &str = "attention";

// ============================================================================
// Source Key Constants
// ============================================================================

/// Source: Candidate devices detected (USB drives ready for preparation)
pub const NOTIF_SOURCE_CANDIDATES: &str = "candidates";

/// Source: Orphaned containers found (zen-offering-* without registry entry)
pub const NOTIF_SOURCE_ORPHAN_CONTAINERS: &str = "orphan-containers";

/// Source: New companion discovered
pub const NOTIF_SOURCE_COMPANION_NEW: &str = "companions.new";

/// Source: Nourishment updates pending
pub const NOTIF_SOURCE_NOURISHMENT: &str = "nourishment.pending";

/// Source: Offerings with degraded health
pub const NOTIF_SOURCE_OFFERINGS_DEGRADED: &str = "offerings.degraded";

/// Source: Adopted service went offline
pub const NOTIF_SOURCE_ADOPTED_OFFLINE: &str = "adopted.offline";

/// Source: Companion process crashed
pub const NOTIF_SOURCE_COMPANION_CRASHED: &str = "companions.crashed";

/// Source: Seed bank went offline
pub const NOTIF_SOURCE_STORAGE_OFFLINE: &str = "storage.offline";

/// Source: System resources critical (disk/memory)
pub const NOTIF_SOURCE_SYSTEM_CRITICAL: &str = "system.critical";

// ============================================================================
// Notification Registry
// ============================================================================

/// Registry for tracking notification state from multiple subsystems
///
/// Thread-safe registry that allows subsystems to set/clear their notification
/// state independently. Compiles to unique tags for topology chirps.
///
/// ## Thread Safety
///
/// Uses `RwLock` internally - safe to share across async tasks.
/// Read-heavy workload (compile on chirp, set/clear less frequent).
#[derive(Debug, Default)]
pub struct NotificationRegistry {
    entries: RwLock<HashMap<String, NotificationTag>>,
}

impl NotificationRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Set a notification for a source
    ///
    /// Overwrites any existing notification for this source.
    ///
    /// # Arguments
    ///
    /// * `source` - Source key (use NOTIF_SOURCE_* constants)
    /// * `tag` - Notification tag type
    pub fn set(&self, source: &str, tag: NotificationTag) {
        let mut entries = self.entries.write().expect("lock poisoned");
        let prev = entries.insert(source.to_string(), tag);

        if prev != Some(tag) {
            tracing::debug!(
                source = source,
                tag = %tag,
                "Notification set"
            );
        }
    }

    /// Clear a notification for a source
    ///
    /// No-op if source doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `source` - Source key to clear
    pub fn clear(&self, source: &str) {
        let mut entries = self.entries.write().expect("lock poisoned");
        if entries.remove(source).is_some() {
            tracing::debug!(source = source, "Notification cleared");
        }
    }

    /// Conditionally set or clear based on a boolean condition
    ///
    /// Convenience method for common pattern:
    /// ```ignore
    /// if condition {
    ///     registry.set(source, tag);
    /// } else {
    ///     registry.clear(source);
    /// }
    /// ```
    pub fn set_if(&self, source: &str, tag: NotificationTag, condition: bool) {
        if condition {
            self.set(source, tag);
        } else {
            self.clear(source);
        }
    }

    /// Compile unique tags for chirping
    ///
    /// Returns deduplicated, sorted list of tag strings.
    /// This is the format sent in topology chirps.
    pub fn compile(&self) -> Vec<String> {
        let entries = self.entries.read().expect("lock poisoned");

        let mut tags: Vec<String> = entries.values().map(|t| t.as_str().to_string()).collect();

        // Deduplicate and sort for deterministic output
        tags.sort();
        tags.dedup();
        tags
    }

    /// Check if any notifications are registered
    pub fn is_empty(&self) -> bool {
        let entries = self.entries.read().expect("lock poisoned");
        entries.is_empty()
    }

    /// Get count of registered sources
    pub fn len(&self) -> usize {
        let entries = self.entries.read().expect("lock poisoned");
        entries.len()
    }

    /// Check if a specific tag type is present
    pub fn has_tag(&self, tag: NotificationTag) -> bool {
        let entries = self.entries.read().expect("lock poisoned");
        entries.values().any(|t| *t == tag)
    }

    /// Get all entries (for debugging/inspection)
    pub fn entries(&self) -> HashMap<String, NotificationTag> {
        let entries = self.entries.read().expect("lock poisoned");
        entries.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_compile() {
        let registry = NotificationRegistry::new();

        registry.set(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity);
        registry.set(NOTIF_SOURCE_OFFERINGS_DEGRADED, NotificationTag::Attention);

        let tags = registry.compile();
        assert_eq!(tags, vec!["attention", "opportunity"]);
    }

    #[test]
    fn test_deduplication() {
        let registry = NotificationRegistry::new();

        // Multiple sources with same tag
        registry.set(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity);
        registry.set(NOTIF_SOURCE_COMPANION_NEW, NotificationTag::Opportunity);

        let tags = registry.compile();
        assert_eq!(tags, vec!["opportunity"]);
    }

    #[test]
    fn test_clear() {
        let registry = NotificationRegistry::new();

        registry.set(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity);
        registry.set(NOTIF_SOURCE_OFFERINGS_DEGRADED, NotificationTag::Attention);

        // Clear one source
        registry.clear(NOTIF_SOURCE_CANDIDATES);

        let tags = registry.compile();
        assert_eq!(tags, vec!["attention"]);
    }

    #[test]
    fn test_set_if() {
        let registry = NotificationRegistry::new();

        registry.set_if(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity, true);
        assert!(registry.has_tag(NotificationTag::Opportunity));

        registry.set_if(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity, false);
        assert!(!registry.has_tag(NotificationTag::Opportunity));
    }

    #[test]
    fn test_tag_survives_partial_clear() {
        let registry = NotificationRegistry::new();

        // Two sources contribute same tag
        registry.set(NOTIF_SOURCE_CANDIDATES, NotificationTag::Opportunity);
        registry.set(NOTIF_SOURCE_COMPANION_NEW, NotificationTag::Opportunity);

        // Clear one - tag should remain
        registry.clear(NOTIF_SOURCE_CANDIDATES);

        assert!(registry.has_tag(NotificationTag::Opportunity));

        // Clear second - tag should be gone
        registry.clear(NOTIF_SOURCE_COMPANION_NEW);

        assert!(!registry.has_tag(NotificationTag::Opportunity));
    }
}
