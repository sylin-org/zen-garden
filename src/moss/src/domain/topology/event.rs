//! Topology domain events.
//!
//! `TopologyChanged` fires on **interesting transitions only** — not on
//! every `upsert_from_chirp`. A peer refresh of an unchanged entry
//! produces no event; only status transitions (Online ↔ Offline), new
//! discoveries, explicit forgets, maintenance evictions, and self-entry
//! chirps fire.
//!
//! See ARCH-0020 §`TopologyChanged` event for the full design.

use garden_common::TopologyEntry;

#[derive(Debug, Clone)]
pub enum TopologyChanged {
    /// A stone was discovered for the first time.
    StoneDiscovered { stone: Box<TopologyEntry> },

    /// A known stone transitioned from Offline to Online.
    StoneOnline {
        stone_id: String,
        stone_name: String,
    },

    /// A known stone transitioned from Online to Offline (via maintenance
    /// timeout or explicit goodbye).
    StoneOffline {
        stone_id: String,
        stone_name: String,
    },

    /// A stone was explicitly forgotten by operator action.
    StoneForgotten { stone_name: String },

    /// A stone was evicted by maintenance after exceeding the TTL.
    StoneEvicted {
        stone_id: String,
        stone_name: String,
    },

    /// The local self-entry was chirped to the garden.
    SelfEntryChirped {
        stone_id: String,
        stone_name: String,
    },
}

impl TopologyChanged {
    pub fn kind(&self) -> ChangeKind {
        match self {
            TopologyChanged::StoneDiscovered { .. } => ChangeKind::Discovered,
            TopologyChanged::StoneOnline { .. } => ChangeKind::Online,
            TopologyChanged::StoneOffline { .. } => ChangeKind::Offline,
            TopologyChanged::StoneForgotten { .. } => ChangeKind::Forgotten,
            TopologyChanged::StoneEvicted { .. } => ChangeKind::Evicted,
            TopologyChanged::SelfEntryChirped { .. } => ChangeKind::Chirped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Discovered,
    Online,
    Offline,
    Forgotten,
    Evicted,
    Chirped,
}

impl ChangeKind {
    pub fn name(self) -> &'static str {
        match self {
            ChangeKind::Discovered => "discovered",
            ChangeKind::Online => "online",
            ChangeKind::Offline => "offline",
            ChangeKind::Forgotten => "forgotten",
            ChangeKind::Evicted => "evicted",
            ChangeKind::Chirped => "chirped",
        }
    }

    pub const ALL_NAMES: &'static [&'static str] = &[
        "discovered",
        "online",
        "offline",
        "forgotten",
        "evicted",
        "chirped",
    ];
}
