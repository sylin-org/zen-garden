//! The garden frame — a stone's self-presentation, spoken on the wire,
//! held in the topology cache, rendered by HTTP: ONE shape, many mouths.
//!
//! Law (CODE-RULES P3 "records are paths"): rootspace holds sections;
//! sections hold facts; every nesting level is a nameable noun. Fields are
//! grouped by what kind of truth they carry:
//!
//!   stone     — WHO speaks (identity; immutable across a boot)
//!   presence  — WHAT it claims right now (health, membership)
//!   services  — its offering inventory (capped items + declared totals)
//!   meta      — frame housekeeping (schema marker, boot identity, seq)
//!
//! Reception facts (`received`) are NOT part of the spoken frame's meaning:
//! senders emit placeholders; listeners overwrite them. The cache holds
//! announced truth separately from what we saw.
//!
//! v0 compatibility is RETIRED (v1 owns its room: group/port/namespace).
//! These shapes are the canonical wire from now on; the fixture suite pins
//! THIS spelling, and consumers across wire/HTTP/cache/rake render it
//! identically (charter bet B1, honored literally).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// How to reach a stone's HTTP surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerAddress {
    /// LAN-routable IP.
    pub ip: IpAddr,
    /// HTTP port.
    pub port: u16,
    /// HTTPS port when pond security is active; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_port: Option<u16>,
}

/// The speaking stone: identity and reachability. Immutable across a boot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stone {
    /// Persistent stone identity (GUIDv7).
    pub id: String,
    /// Human-facing name; also the mDNS label.
    pub name: String,
    /// The moss build speaking.
    pub moss: Moss,
    /// How to reach it, plus wake-on-LAN material.
    pub network: Network,
}

/// Software identity of the resident daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Moss {
    /// Version, self-reported.
    pub version: String,
}

/// Everything needed to open a connection — and to wake it later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Network {
    /// LAN-routable address of the HTTP surface.
    pub address: PeerAddress,
    /// MAC for wake-on-LAN; absent where unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
}

/// The frame's claims about right now.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Presence {
    /// Self-assessed vitality (glossary::health).
    pub health: String,
    /// Membership state as spoken (glossary::presence).
    pub status: String,
}

/// One offering a stone runs, as seen in presence. Identity fields speak
/// FQN verbatim (ADR-0003); moniker suppression is a rendering concern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ServiceEntry {
    /// Stable identity of the offering instance; survives renames.
    #[serde(default)]
    pub offering_id: String,
    /// Fully-qualified offering name (e.g. `memcached::default`).
    pub name: String,
    /// Catalog stem this instance was planted from (provenance).
    pub stem: String,
    /// Catalog category (glossary noun).
    pub category: String,
    /// How the instance is doing: runtime status + orchestration role.
    pub state: ServiceState,
    /// Actual host ports by name ("default", "management"...) — populated
    /// only where remapped from manifest defaults (PORT-0001 inherited).
    /// Absent/empty = defaults stand.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub ports: std::collections::HashMap<String, u16>,
}

/// Runtime condition of one offering instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ServiceState {
    /// running / stopped / degraded (glossary).
    pub status: String,
    /// Orchestration role when active: primary | replica | joining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// A domain inventory block: capped items, declared totals, one revision.
/// One shape per garden domain — services today, banks with the storage
/// slice (ADR-0005 §8) — so new domains extend the frame without touching
/// rootspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Inventory<T> {
    /// Monotonic per-boot generation of this domain's set. The merge
    /// function: frames compare revisions, mismatches heal by rich ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<u64>,
    /// Total items hosted when `items` is truncated to the wire cap;
    /// absent = everything fit (truncation declared, never silent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    /// The (possibly capped) items themselves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<T>,
}

/// Wire cap on inventory items per frame (ADR-0004 §1). Keeps the whole
/// envelope safely inside the <4 KB budget with signature headroom.
pub const INVENTORY_CAP: usize = 24;

/// Frame housekeeping: schema identity and ordering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FrameMeta {
    /// Wire schema marker; [`crate::consts::PROTO_V1`] when spoken by v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto: Option<String>,
    /// Identity of this boot — peers distinguish restart from heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
    /// Monotonic chirp counter for this boot — gap detection, ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

/// Reception facts: when WE first/last heard from this stone. Senders emit
/// placeholders; every listener overwrites both — they describe the
/// relationship, not the speaker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Reception {
    /// First-seen timestamp (listeners overwrite).
    pub discovered_at: DateTime<Utc>,
    /// Last-seen timestamp (listeners overwrite).
    pub last_seen: DateTime<Utc>,
}

/// The garden frame: one canonical shape spoken on the wire, held in the
/// topology cache, and projected by HTTP surfaces. Sections, not a flat
/// field zoo. (No Default: a frame without a speaking stone is meaningless.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChirpFrame {
    /// WHO speaks: identity and reachability.
    pub stone: Stone,
    /// WHAT it claims right now.
    pub presence: Presence,
    /// Its offering inventory (rev + capped items + declared total).
    #[serde(default)]
    pub services: Inventory<ServiceEntry>,
    /// Frame housekeeping.
    #[serde(default)]
    pub meta: FrameMeta,
    /// Reception facts — senders emit placeholders, listeners overwrite.
    pub received: Reception,
}

impl ChirpFrame {
    /// A frame that says: this stone answered a discovery ask. Health is
    /// honest-not-claimed (`starting` hint — capabilities unknown until its
    /// own chirp arrives, W1 precedent).
    pub fn answered(name: impl Into<String>, address: PeerAddress, version: impl Into<String>) -> Self {
        use garden_glossary::{health, presence};
        let now = Utc::now();
        Self {
            stone: Stone {
                id: String::new(), // unknown until the stone's own chirp arrives
                name: name.into(),
                moss: Moss { version: version.into() },
                network: Network { address, mac: None },
            },
            presence: Presence {
                health: health::STARTING.into(),
                status: presence::ONLINE.into(),
            },
            services: Inventory::default(),
            meta: FrameMeta::default(),
            received: Reception { discovered_at: now, last_seen: now },
        }
    }
}
