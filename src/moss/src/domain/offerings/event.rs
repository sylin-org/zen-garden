//! Offerings aggregate event — emitted on every mutation.
//!
//! Distinct from `OfferingEvent` (lifecycle transitions of real-world services
//! like container started/stopped). `OfferingsChanged` describes the
//! registry-membership mutation that just happened on this stone's offerings
//! aggregate: an upsert, a removal, a promote, a demote, etc.
//!
//! Subscribers (tool registry projection, topology/chirp sync) react by
//! re-reading the aggregate snapshot and rebuilding their projection.

use chrono::{DateTime, Utc};

/// Kind of mutation that was applied to the aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// An offering was inserted or updated in the active pool.
    Upserted,
    /// An offering was removed from the active pool.
    Removed,
    /// An offering's fields were updated in place.
    Updated,
    /// An adopted candidate was promoted to the active pool.
    Promoted,
    /// An adopted offering was demoted back to the candidates pool.
    Demoted,
    /// The active pool was wholesale replaced.
    Replaced,
    /// Duplicate offerings (by FQN) were coalesced.
    Coalesced,
    /// A batch mutation touched one or more offerings.
    BatchUpdated,
}

impl ChangeKind {
    /// Whether this change should trigger an immediate UDP chirp.
    ///
    /// Currently every kind chirps. Future optimization could add a
    /// `QuietUpdate` variant for in-place mutations that don't affect
    /// topology visibility.
    pub fn should_chirp(self) -> bool {
        true
    }
}

/// Event published by the Offerings aggregate on every mutation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OfferingsChanged {
    /// What kind of mutation happened.
    pub kind: ChangeKind,
    /// Offering IDs affected by this mutation. May be empty for batch
    /// operations that touched zero items (in which case the event is
    /// still emitted so subscribers can reconcile).
    pub affected: Vec<String>,
    /// Wall-clock timestamp of the mutation.
    pub timestamp: DateTime<Utc>,
}

impl OfferingsChanged {
    pub(super) fn new(kind: ChangeKind, affected: Vec<String>) -> Self {
        Self {
            kind,
            affected,
            timestamp: Utc::now(),
        }
    }
}
