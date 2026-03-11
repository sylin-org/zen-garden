//! Nurturing domain - A/B local backup management
//!
//! Provides local A/B backup rotation for offerings:
//! - 2 slots per offering (A and B)
//! - Automatic rotation (oldest slot gets overwritten)
//! - Keyed by offering_id (survives renames)
//!
//! This module builds on the harvest infrastructure but adds:
//! - Slot management (A/B rotation)
//! - offering_id-based keying
//! - Quick rollback capability
//!
//! # Architecture
//! - Uses existing HarvestStore for actual backup operations
//! - NurturingManifest wraps HarvestManifest with slot metadata
//! - Maintains index of offering_id -> slots mapping

use crate::domain::harvest::HarvestManifest;
use chrono::{DateTime, Utc};
use garden_common::manifests::Offering as OfferingManifest;
use garden_common::storage::MemoriesOfferingManifest;
use garden_common::types::Offering;
use serde::{Deserialize, Serialize};

/// A/B slot identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NurturingSlot {
    /// Primary slot
    A,
    /// Secondary slot
    B,
}

impl NurturingSlot {
    /// Get the other slot (for rotation)
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }

    /// Get slot name for display/storage
    pub fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

impl std::fmt::Display for NurturingSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Nurturing snapshot - a harvest in a specific slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NurturingSnapshot {
    /// Slot this snapshot occupies
    pub slot: NurturingSlot,
    /// Offering ID (GUIDv7) - survives renames
    pub offering_id: String,
    /// Offering name at time of snapshot
    pub offering_name: String,
    /// Underlying harvest ID
    pub harvest_id: String,
    /// When this snapshot was created
    pub created_at: DateTime<Utc>,
    /// Size in bytes
    pub size_bytes: u64,
    /// Whether this is the current/latest snapshot
    pub is_current: bool,
}

impl NurturingSnapshot {
    /// Create a new snapshot from a harvest
    pub fn from_harvest(
        harvest: &HarvestManifest,
        slot: NurturingSlot,
        offering_id: &str,
        is_current: bool,
    ) -> Self {
        Self {
            slot,
            offering_id: offering_id.to_string(),
            offering_name: harvest.offering.clone(),
            harvest_id: harvest.id.clone(),
            created_at: harvest.created_at,
            size_bytes: harvest.total_size_bytes(),
            is_current,
        }
    }
}

/// A/B slots for a single offering
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfferingSlots {
    /// Offering ID (GUIDv7) - the key
    pub offering_id: String,
    /// Current offering name (may change)
    pub offering_name: String,
    /// Slot A snapshot (if any)
    pub slot_a: Option<NurturingSnapshot>,
    /// Slot B snapshot (if any)
    pub slot_b: Option<NurturingSnapshot>,
}

impl OfferingSlots {
    /// Create new empty slots for an offering
    pub fn new(offering_id: &str, offering_name: &str) -> Self {
        Self {
            offering_id: offering_id.to_string(),
            offering_name: offering_name.to_string(),
            slot_a: None,
            slot_b: None,
        }
    }

    /// Get slot by identifier
    pub fn get(&self, slot: NurturingSlot) -> Option<&NurturingSnapshot> {
        match slot {
            NurturingSlot::A => self.slot_a.as_ref(),
            NurturingSlot::B => self.slot_b.as_ref(),
        }
    }

    /// Get mutable slot by identifier
    pub fn get_mut(&mut self, slot: NurturingSlot) -> &mut Option<NurturingSnapshot> {
        match slot {
            NurturingSlot::A => &mut self.slot_a,
            NurturingSlot::B => &mut self.slot_b,
        }
    }

    /// Set a snapshot in a slot
    pub fn set(&mut self, slot: NurturingSlot, snapshot: NurturingSnapshot) {
        // Mark the new snapshot as current, demote the other
        let mut snapshot = snapshot;
        snapshot.is_current = true;

        match slot {
            NurturingSlot::A => {
                if let Some(ref mut b) = self.slot_b {
                    b.is_current = false;
                }
                self.slot_a = Some(snapshot);
            }
            NurturingSlot::B => {
                if let Some(ref mut a) = self.slot_a {
                    a.is_current = false;
                }
                self.slot_b = Some(snapshot);
            }
        }
    }

    /// Determine which slot to use next (for rotation)
    ///
    /// Strategy:
    /// 1. If both empty, use A
    /// 2. If one empty, use the empty one
    /// 3. If both filled, use the older one
    pub fn next_slot(&self) -> NurturingSlot {
        match (&self.slot_a, &self.slot_b) {
            (None, None) => NurturingSlot::A,
            (None, Some(_)) => NurturingSlot::A,
            (Some(_), None) => NurturingSlot::B,
            (Some(a), Some(b)) => {
                // Overwrite the older one
                if a.created_at <= b.created_at {
                    NurturingSlot::A
                } else {
                    NurturingSlot::B
                }
            }
        }
    }

    /// Get the current (most recent) snapshot
    pub fn current(&self) -> Option<&NurturingSnapshot> {
        match (&self.slot_a, &self.slot_b) {
            (Some(a), _) if a.is_current => Some(a),
            (_, Some(b)) if b.is_current => Some(b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => {
                // Fallback: return the newer one
                if a.created_at >= b.created_at {
                    Some(a)
                } else {
                    Some(b)
                }
            }
            (None, None) => None,
        }
    }

    /// Get the previous (rollback) snapshot
    pub fn previous(&self) -> Option<&NurturingSnapshot> {
        match (&self.slot_a, &self.slot_b) {
            (Some(a), _) if !a.is_current => Some(a),
            (_, Some(b)) if !b.is_current => Some(b),
            _ => None,
        }
    }

    /// Get total size of both slots
    pub fn total_size(&self) -> u64 {
        self.slot_a.as_ref().map(|s| s.size_bytes).unwrap_or(0)
            + self.slot_b.as_ref().map(|s| s.size_bytes).unwrap_or(0)
    }

    /// Check if any slots are filled
    pub fn has_snapshots(&self) -> bool {
        self.slot_a.is_some() || self.slot_b.is_some()
    }

    /// Get harvest IDs to delete when removing this offering's slots
    pub fn harvest_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(a) = &self.slot_a {
            ids.push(a.harvest_id.clone());
        }
        if let Some(b) = &self.slot_b {
            ids.push(b.harvest_id.clone());
        }
        ids
    }
}

/// Nurturing index - maps offering_id to slots
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NurturingIndex {
    /// Version for future migrations
    pub version: u32,
    /// All offerings with their A/B slots
    pub offerings: Vec<OfferingSlots>,
}

impl NurturingIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            version: 1,
            offerings: Vec::new(),
        }
    }

    /// Get slots for an offering by ID
    pub fn get(&self, offering_id: &str) -> Option<&OfferingSlots> {
        self.offerings.iter().find(|o| o.offering_id == offering_id)
    }

    /// Get mutable slots for an offering by ID
    pub fn get_mut(&mut self, offering_id: &str) -> Option<&mut OfferingSlots> {
        self.offerings
            .iter_mut()
            .find(|o| o.offering_id == offering_id)
    }

    /// Get or create slots for an offering
    pub fn get_or_create(&mut self, offering_id: &str, offering_name: &str) -> &mut OfferingSlots {
        if !self.offerings.iter().any(|o| o.offering_id == offering_id) {
            self.offerings
                .push(OfferingSlots::new(offering_id, offering_name));
        }
        self.offerings
            .iter_mut()
            .find(|o| o.offering_id == offering_id)
            .unwrap()
    }

    /// Remove slots for an offering
    pub fn remove(&mut self, offering_id: &str) -> Option<OfferingSlots> {
        if let Some(pos) = self
            .offerings
            .iter()
            .position(|o| o.offering_id == offering_id)
        {
            Some(self.offerings.remove(pos))
        } else {
            None
        }
    }

    /// Get total storage used by all nurturing snapshots
    pub fn total_size(&self) -> u64 {
        self.offerings.iter().map(|o| o.total_size()).sum()
    }

    /// List all offerings with snapshots
    pub fn list_offerings(&self) -> Vec<&OfferingSlots> {
        self.offerings
            .iter()
            .filter(|o| o.has_snapshots())
            .collect()
    }
}

/// Result of a nurturing operation
#[derive(Debug, Clone, Serialize)]
pub struct NurturingResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Offering ID
    pub offering_id: String,
    /// Offering name
    pub offering_name: String,
    /// Slot used (A or B)
    pub slot: NurturingSlot,
    /// Harvest ID created
    pub harvest_id: String,
    /// Previous harvest that was replaced (if any)
    pub replaced_harvest_id: Option<String>,
    /// Snapshot size in bytes
    pub size_bytes: u64,
    /// Human-readable message
    pub message: String,
}

// ============================================================================
// Remote Nurturing Types (Seed Bank Integration)
// ============================================================================

/// Remote snapshot stored on a seed bank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSnapshot {
    /// Offering ID (GUIDv7)
    pub offering_id: String,
    /// Offering name at time of snapshot
    pub offering_name: String,
    /// Harvest ID
    pub harvest_id: String,
    /// Seed bank ID where stored
    pub seed_bank_id: String,
    /// Seed bank name
    pub storage_name: String,
    /// Stone that created this snapshot
    pub source_stone: String,
    /// When this snapshot was created
    pub created_at: DateTime<Utc>,
    /// Size in bytes
    pub size_bytes: u64,
    /// Object key in seed bank storage
    pub object_key: String,
}

/// Result of a remote replication operation
#[derive(Debug, Clone, Serialize)]
pub struct ReplicationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Offering ID
    pub offering_id: String,
    /// Harvest ID replicated
    pub harvest_id: String,
    /// Seed bank ID where stored
    pub seed_bank_id: String,
    /// Seed bank name
    pub storage_name: String,
    /// Size in bytes transferred
    pub size_bytes: u64,
    /// Harvest IDs pruned due to retention policy
    pub pruned_harvest_ids: Vec<String>,
    /// Human-readable message
    pub message: String,
}

/// Default retention policy: 5 slots per offering
pub const DEFAULT_RETENTION_SLOTS: usize = 5;

/// Remote snapshots index for a seed bank
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteNurturingIndex {
    /// Version for future migrations
    pub version: u32,
    /// Seed bank ID
    pub seed_bank_id: String,
    /// All remote snapshots on this seed bank
    pub snapshots: Vec<RemoteSnapshot>,
    /// Retention slots per offering (default 5)
    #[serde(default = "default_retention_slots")]
    pub retention_slots: usize,
}

// ============================================================================
// Hydration Helpers
// ============================================================================

/// Build a hydration manifest for a runtime offering.
pub fn build_memories_manifest(
    offering: &Offering,
    manifest: Option<OfferingManifest>,
    stone_id: &str,
    stone_name: &str,
) -> MemoriesOfferingManifest {
    MemoriesOfferingManifest::from_offering(offering, manifest, stone_id, stone_name)
}

fn default_retention_slots() -> usize {
    DEFAULT_RETENTION_SLOTS
}

impl RemoteNurturingIndex {
    /// Create a new empty index for a seed bank
    pub fn new(seed_bank_id: &str) -> Self {
        Self {
            version: 1,
            seed_bank_id: seed_bank_id.to_string(),
            snapshots: Vec::new(),
            retention_slots: DEFAULT_RETENTION_SLOTS,
        }
    }

    /// Create with custom retention slots
    pub fn with_retention(seed_bank_id: &str, retention_slots: usize) -> Self {
        Self {
            version: 1,
            seed_bank_id: seed_bank_id.to_string(),
            snapshots: Vec::new(),
            retention_slots: retention_slots.max(1), // At least 1 slot
        }
    }

    /// Get snapshots for a specific offering, sorted newest first
    pub fn get_for_offering(&self, offering_id: &str) -> Vec<&RemoteSnapshot> {
        let mut snapshots: Vec<_> = self
            .snapshots
            .iter()
            .filter(|s| s.offering_id == offering_id)
            .collect();
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        snapshots
    }

    /// Add a snapshot (maintains order by created_at, newest first)
    pub fn add(&mut self, snapshot: RemoteSnapshot) {
        self.snapshots.push(snapshot);
        self.snapshots
            .sort_by(|a, b| b.created_at.cmp(&a.created_at));
    }

    /// Add a snapshot and enforce retention policy for that offering
    ///
    /// Returns the snapshots that were pruned (if any) for cleanup.
    pub fn add_with_retention(&mut self, snapshot: RemoteSnapshot) -> Vec<RemoteSnapshot> {
        let offering_id = snapshot.offering_id.clone();
        self.add(snapshot);
        self.prune_offering(&offering_id)
    }

    /// Prune excess snapshots for an offering based on retention policy
    ///
    /// Returns the pruned snapshots (for deletion from storage).
    pub fn prune_offering(&mut self, offering_id: &str) -> Vec<RemoteSnapshot> {
        // Get indices of snapshots for this offering, sorted by created_at (newest first)
        let mut offering_indices: Vec<(usize, &RemoteSnapshot)> = self
            .snapshots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.offering_id == offering_id)
            .collect();
        offering_indices.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        // If within retention limit, nothing to prune
        if offering_indices.len() <= self.retention_slots {
            return Vec::new();
        }

        // Collect indices to remove (the older ones beyond retention limit)
        let indices_to_remove: Vec<usize> = offering_indices
            .iter()
            .skip(self.retention_slots)
            .map(|(idx, _)| *idx)
            .collect();

        // Remove in reverse order to preserve indices
        let mut pruned = Vec::new();
        let mut sorted_indices = indices_to_remove.clone();
        sorted_indices.sort_by(|a, b| b.cmp(a)); // Descending order

        for idx in sorted_indices {
            pruned.push(self.snapshots.remove(idx));
        }

        pruned
    }

    /// Get all unique offering IDs in this index
    pub fn offering_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .snapshots
            .iter()
            .map(|s| s.offering_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Get count of snapshots per offering
    pub fn snapshot_counts(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for s in &self.snapshots {
            *counts.entry(s.offering_id.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Remove a snapshot by harvest_id
    pub fn remove(&mut self, harvest_id: &str) -> Option<RemoteSnapshot> {
        if let Some(pos) = self
            .snapshots
            .iter()
            .position(|s| s.harvest_id == harvest_id)
        {
            Some(self.snapshots.remove(pos))
        } else {
            None
        }
    }

    /// Get total size of all snapshots
    pub fn total_size(&self) -> u64 {
        self.snapshots.iter().map(|s| s.size_bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_rotation() {
        let mut slots = OfferingSlots::new("test-id", "test");

        // First slot should be A
        assert_eq!(slots.next_slot(), NurturingSlot::A);

        // After A is filled, next should be B
        slots.slot_a = Some(NurturingSnapshot {
            slot: NurturingSlot::A,
            offering_id: "test-id".into(),
            offering_name: "test".into(),
            harvest_id: "harvest-a".into(),
            created_at: Utc::now(),
            size_bytes: 100,
            is_current: true,
        });
        assert_eq!(slots.next_slot(), NurturingSlot::B);

        // After B is filled with newer timestamp, next should be A (older)
        slots.slot_b = Some(NurturingSnapshot {
            slot: NurturingSlot::B,
            offering_id: "test-id".into(),
            offering_name: "test".into(),
            harvest_id: "harvest-b".into(),
            created_at: Utc::now(),
            size_bytes: 200,
            is_current: true,
        });
        assert_eq!(slots.next_slot(), NurturingSlot::A);
    }

    #[test]
    fn test_current_and_previous() {
        let mut slots = OfferingSlots::new("test-id", "test");

        // No snapshots initially
        assert!(slots.current().is_none());
        assert!(slots.previous().is_none());

        // Add first snapshot
        slots.set(
            NurturingSlot::A,
            NurturingSnapshot {
                slot: NurturingSlot::A,
                offering_id: "test-id".into(),
                offering_name: "test".into(),
                harvest_id: "harvest-a".into(),
                created_at: Utc::now(),
                size_bytes: 100,
                is_current: false, // will be set to true by set()
            },
        );

        assert!(slots.current().is_some());
        assert_eq!(slots.current().unwrap().harvest_id, "harvest-a");
        assert!(slots.previous().is_none()); // only one slot filled

        // Add second snapshot
        slots.set(
            NurturingSlot::B,
            NurturingSnapshot {
                slot: NurturingSlot::B,
                offering_id: "test-id".into(),
                offering_name: "test".into(),
                harvest_id: "harvest-b".into(),
                created_at: Utc::now(),
                size_bytes: 200,
                is_current: false,
            },
        );

        assert_eq!(slots.current().unwrap().harvest_id, "harvest-b");
        assert_eq!(slots.previous().unwrap().harvest_id, "harvest-a");
    }

    #[test]
    fn test_index_operations() {
        let mut index = NurturingIndex::new();

        // Get or create
        let slots = index.get_or_create("offering-123", "mongodb");
        assert_eq!(slots.offering_id, "offering-123");
        assert_eq!(slots.offering_name, "mongodb");

        // Get existing
        assert!(index.get("offering-123").is_some());
        assert!(index.get("nonexistent").is_none());

        // Remove
        let removed = index.remove("offering-123");
        assert!(removed.is_some());
        assert!(index.get("offering-123").is_none());
    }

    #[test]
    fn test_remote_retention_policy() {
        use chrono::Duration;

        let mut index = RemoteNurturingIndex::with_retention("seed-bank-1", 3);
        assert_eq!(index.retention_slots, 3);

        let base_time = Utc::now();

        // Add 5 snapshots for the same offering
        for i in 0..5 {
            let snapshot = RemoteSnapshot {
                offering_id: "mongodb-id".into(),
                offering_name: "mongodb".into(),
                harvest_id: format!("harvest-{}", i),
                seed_bank_id: "seed-bank-1".into(),
                storage_name: "portable-backup".into(),
                source_stone: "stone-01".into(),
                created_at: base_time + Duration::hours(i as i64),
                size_bytes: 1000,
                object_key: format!("mongodb-id/harvest-{}.tar.gz", i),
            };
            let pruned = index.add_with_retention(snapshot);

            // First 3 additions should not prune anything
            if i < 3 {
                assert!(pruned.is_empty(), "Unexpected pruning at snapshot {}", i);
            } else {
                // 4th and 5th should prune the oldest
                assert_eq!(pruned.len(), 1, "Expected 1 pruned at snapshot {}", i);
            }
        }

        // Should have exactly 3 snapshots (retention limit)
        assert_eq!(index.snapshots.len(), 3);

        // Should be the 3 newest (harvest-4, harvest-3, harvest-2)
        let offering_snapshots = index.get_for_offering("mongodb-id");
        assert_eq!(offering_snapshots.len(), 3);
        assert_eq!(offering_snapshots[0].harvest_id, "harvest-4");
        assert_eq!(offering_snapshots[1].harvest_id, "harvest-3");
        assert_eq!(offering_snapshots[2].harvest_id, "harvest-2");
    }

    #[test]
    fn test_remote_retention_multiple_offerings() {
        use chrono::Duration;

        let mut index = RemoteNurturingIndex::with_retention("seed-bank-1", 2);
        let base_time = Utc::now();

        // Add 3 snapshots for offering A
        for i in 0..3 {
            let snapshot = RemoteSnapshot {
                offering_id: "offering-a".into(),
                offering_name: "mongodb".into(),
                harvest_id: format!("a-harvest-{}", i),
                seed_bank_id: "seed-bank-1".into(),
                storage_name: "portable-backup".into(),
                source_stone: "stone-01".into(),
                created_at: base_time + Duration::hours(i as i64),
                size_bytes: 1000,
                object_key: format!("offering-a/a-harvest-{}.tar.gz", i),
            };
            index.add_with_retention(snapshot);
        }

        // Add 3 snapshots for offering B
        for i in 0..3 {
            let snapshot = RemoteSnapshot {
                offering_id: "offering-b".into(),
                offering_name: "redis".into(),
                harvest_id: format!("b-harvest-{}", i),
                seed_bank_id: "seed-bank-1".into(),
                storage_name: "portable-backup".into(),
                source_stone: "stone-01".into(),
                created_at: base_time + Duration::hours(i as i64),
                size_bytes: 500,
                object_key: format!("offering-b/b-harvest-{}.tar.gz", i),
            };
            index.add_with_retention(snapshot);
        }

        // Each offering should have exactly 2 snapshots
        assert_eq!(index.get_for_offering("offering-a").len(), 2);
        assert_eq!(index.get_for_offering("offering-b").len(), 2);
        assert_eq!(index.snapshots.len(), 4); // 2 + 2

        // Verify the newest snapshots are kept
        let a_snapshots = index.get_for_offering("offering-a");
        assert_eq!(a_snapshots[0].harvest_id, "a-harvest-2");
        assert_eq!(a_snapshots[1].harvest_id, "a-harvest-1");

        let b_snapshots = index.get_for_offering("offering-b");
        assert_eq!(b_snapshots[0].harvest_id, "b-harvest-2");
        assert_eq!(b_snapshots[1].harvest_id, "b-harvest-1");
    }

    #[test]
    fn test_default_retention_is_five() {
        let index = RemoteNurturingIndex::new("seed-bank-1");
        assert_eq!(index.retention_slots, DEFAULT_RETENTION_SLOTS);
        assert_eq!(index.retention_slots, 5);
    }
}
