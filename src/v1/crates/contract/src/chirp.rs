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
/// The unit of the inventory map (A2.1) and of framer quantization (A2.3):
/// a block rides WHOLE or waits — partial item lists are forbidden.
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

/// The inventory MAP (ADR-0004 A2.1): rootspace of the frame is closed;
/// garden domains enter here as blocks. Known domains are compile-time
/// fields; unknown domains from newer stones round-trip losslessly through
/// the passthrough map (older stones relay without destroying).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InventoryMap {
    /// Offerings this stone hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub services: Option<Inventory<ServiceEntry>>,
    /// Storage banks this stone holds (ADR-0005 §8; lands with the storage
    /// slice — claimed slot, typed then).
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "banks")]
    pub _banks_slot: Option<serde_json::Value>,
    /// Unknown domains from newer speakers, preserved verbatim.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl InventoryMap {
    /// Build from (domain, block) pairs — the framer's and composer's
    /// natural input shape. A pair keyed `"services"` populates the typed
    /// slot (decoded from JSON); anything else lands in the passthrough.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, serde_json::Value)>) -> Self {
        let mut map = Self::default();
        for (key, value) in pairs {
            map.insert(key, value);
        }
        map
    }

    /// Insert one domain block: `"services"` decodes into the typed slot;
    /// every other key is preserved verbatim in the passthrough map.
    pub fn insert(&mut self, key: String, value: serde_json::Value) {
        if key == "services" {
            let decoded = serde_json::from_value::<Inventory<ServiceEntry>>(value.clone());
            if let Ok(inv) = decoded {
                self.services = Some(inv);
                return;
            }
            // Not decodable as a service block (foreign or future shape):
            // preserve verbatim rather than destroy.
        }
        self.extra.insert(key, value);
    }

    /// Merge `newer` over `self` per-domain by revision (A2.1): absent key
    /// keeps what we have; present block's rev decides. Unknown-domain
    /// blocks merge by their embedded `rev` when comparable.
    pub fn merge_frame(&mut self, newer: &InventoryMap) {
        if let Some(n) = &newer.services {
            let stale = self
                .services
                .as_ref()
                .and_then(|m| m.rev)
                .is_some_and(|old| n.rev.is_some_and(|new| new <= old));
            if !stale {
                self.services = Some(n.clone());
            }
        }
        // Unknown domains: last-writer-wins on whole JSON blocks. Their
        // internal rev comparison is the owning slice's problem (A2.3);
        // relays must not guess semantics they cannot parse.
        for (k, v) in &newer.extra {
            self.extra.insert(k.clone(), v.clone());
        }
    }
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
    /// Quantization position when one announcement spans several frames
    /// (ADR-0004 A2.3). Purely informational: consumers never wait or
    /// reassemble — revs make every frame independently mergeable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Part>,
}

/// Position of this frame within a multi-frame announcement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Part {
    pub n: u32,
    pub of: u32,
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
    /// Its inventory domains (services today; banks next; unknown preserved).
    #[serde(default)]
    pub inventory: InventoryMap,
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
            inventory: InventoryMap::default(),
            meta: FrameMeta::default(),
            received: Reception { discovered_at: now, last_seen: now },
        }
    }
}
