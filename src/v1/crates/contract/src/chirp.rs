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
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Moss {
    /// Version, self-reported.
    pub version: String,
}

/// Everything needed to open a connection — and to wake it later.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Network {
    /// LAN-routable address of the HTTP surface.
    pub address: PeerAddress,
    /// MAC for wake-on-LAN; absent where unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
}

/// The frame's claims about right now.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Presence {
    /// Self-assessed vitality (glossary::health).
    pub health: String,
    /// Membership state as spoken (glossary::presence).
    pub status: String,
}

/// One offering a stone runs, as seen in presence. Identity fields speak
/// FQN verbatim (ADR-0003); moniker suppression is a rendering concern.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    /// Content this instance holds, by capability type ("model" ->
    /// ["llama3"]). Declared in the offering's manifest, observed by the
    /// stone's capability sweep; omitted when the offering declares no
    /// capability types. Capped (MAX_CAPABILITY_ITEMS).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub capabilities: std::collections::HashMap<String, Vec<String>>,
}

/// Runtime condition of one offering instance.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ServiceState {
    /// running / stopped / degraded (glossary).
    pub status: String,
    /// Orchestration role when active: primary | replica | joining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// One storage bank in presence (ADR-0005 §8): logical FQN identity plus
/// the physical device, its state, and roles. Capacity/used are TELEMETRY —
/// they never trigger frames, they ride along (§8.2's anti-spam law); both
/// are optional because "unknown" is honest.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BankEntry {
    /// Logical bank identity, FQN per ADR-0003 (`bank::default` communal,
    /// explicit instances private).
    pub fqn: String,
    /// Physical device identity (GUIDv7, per-device).
    pub device_id: String,
    /// mounted | ejected (glossary::bank).
    pub state: String,
    /// Declared roles (sink today; the set grows with ADR-0005's tiers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// Total bytes, when measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_bytes: Option<u64>,
    /// Used bytes, when measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_bytes: Option<u64>,
}

/// A domain inventory block: capped items, declared totals, one revision.
/// The unit of the inventory map (A2.1) and of framer quantization (A2.3):
/// a block rides WHOLE or waits — partial item lists are forbidden.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InventoryMap {
    /// Offerings this stone hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub services: Option<Inventory<ServiceEntry>>,
    /// Storage banks this stone holds (ADR-0005 §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banks: Option<Inventory<BankEntry>>,
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

    /// Insert one domain block: known keys decode into their typed slots;
    /// every other key is preserved verbatim in the passthrough map.
    pub fn insert(&mut self, key: String, value: serde_json::Value) {
        if key == DOMAIN_SERVICES
            && let Ok(inv) = serde_json::from_value::<Inventory<ServiceEntry>>(value.clone())
        {
            self.services = Some(inv);
            return;
        }
        if key == DOMAIN_BANKS
            && let Ok(inv) = serde_json::from_value::<Inventory<BankEntry>>(value.clone())
        {
            self.banks = Some(inv);
            return;
        }
        // Not decodable as a known block (foreign or future shape):
        // preserve verbatim rather than destroy.
        self.extra.insert(key, value);
    }

    /// True when the map says nothing about any domain (skipped on the wire).
    pub fn is_empty(&self) -> bool {
        self.services.is_none() && self.banks.is_none() && self.extra.is_empty()
    }

    /// Merge `newer` over `self` per-domain by revision (A2.1): absent key
    /// keeps what we have; present block's rev decides. Unknown-domain
    /// blocks merge by their embedded `rev` when comparable.
    pub fn merge_frame(&mut self, newer: &InventoryMap) {
        // Typed domains merge by per-block revision; absent key keeps what
        // we have; a present block's rev speaks. Within ONE rev the lean
        // voice carries the rev without items and the full voice carries
        // the set — so at equal revs the richer block fills the thinner:
        // an equal rev is one generation of truth, and a generation's
        // content only grows. (A lean heartbeat must never wipe what an
        // answer or a song taught; an equal-rev answer must be able to
        // teach a lean-seated stone.)
        if let Some(n) = &newer.services {
            let stale = stale_replace(self.services.as_ref(), n);
            if !stale {
                self.services = Some(n.clone());
            }
        }
        if let Some(n) = &newer.banks {
            let stale = stale_replace(self.banks.as_ref(), n);
            if !stale {
                self.banks = Some(n.clone());
            }
        }
        // Unknown domains: last-writer-wins on whole JSON blocks. Their
        // internal rev comparison is the owning slice's problem (A2.3);
        // relays must not guess semantics they cannot parse.
        for (k, v) in &newer.extra {
            self.extra.insert(k.clone(), v.clone());
        }
    }

    /// The rumor's fill for a freshly-seated peer (the late-joiner's
    /// convergence): the rich boot answer rides the candidate pool, and
    /// the stone's first LEAN frame carries revs without items — so for
    /// each typed domain, apply the rumor's block ONLY where the seated
    /// frame speaks the same generation thin (equal rev, no items) or
    /// stayed silent. A rumor from another generation (an older
    /// incarnation relayed late) must not overwrite the stone's own
    /// first-hand frame.
    pub fn fill_rumor(&mut self, rumor: &InventoryMap) {
        fill_domain(&mut self.services, &rumor.services);
        fill_domain(&mut self.banks, &rumor.banks);
    }
}

/// The per-domain staleness rule behind [`InventoryMap::merge_frame`]:
/// a strictly older rev is stale; at an equal rev a block is stale
/// unless it carries items the stored (thin) block lacks.
fn stale_replace<T>(old: Option<&Inventory<T>>, new: &Inventory<T>) -> bool {
    let Some(old) = old else { return false };
    match (old.rev, new.rev) {
        (Some(o), Some(n)) if n < o => true,
        (Some(o), Some(n)) if n == o => !old.items.is_empty() || new.items.is_empty(),
        _ => false,
    }
}

/// One domain's [`InventoryMap::fill_rumor`] rule: silent keeps silence;
/// same-generation-thin takes the rumor's set; anything else keeps the
/// frame's own word.
fn fill_domain<T: Clone>(dst: &mut Option<Inventory<T>>, rumor: &Option<Inventory<T>>) {
    let Some(r) = rumor else { return };
    let applies = match dst {
        None => true,
        Some(cur) => cur.rev == r.rev && cur.items.is_empty() && !r.items.is_empty(),
    };
    if applies {
        *dst = Some(r.clone());
    }
}

/// The services domain's inventory-map key (A2.1). Wire literal: changing
/// it is a contract change and must fail the fixtures.
pub const DOMAIN_SERVICES: &str = "services";

/// The banks domain's inventory-map key (ADR-0005 §8). Wire literal, same
/// law as [`DOMAIN_SERVICES`].
pub const DOMAIN_BANKS: &str = "banks";

/// Wire cap on inventory items per frame (ADR-0004 §1). Keeps the whole
/// envelope safely inside the <4 KB budget with signature headroom.
pub const INVENTORY_CAP: usize = 24;

/// Frame housekeeping: schema identity and ordering.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
#[derive(schemars::JsonSchema, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Part {
    pub n: u32,
    pub of: u32,
}

/// Reception facts: when WE first/last heard from this stone. Senders emit
/// placeholders; every listener overwrites both — they describe the
/// relationship, not the speaker.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Reception {
    /// First-seen timestamp (listeners overwrite).
    pub discovered_at: DateTime<Utc>,
    /// Last-seen timestamp (listeners overwrite).
    pub last_seen: DateTime<Utc>,
}

/// The garden frame: one canonical shape spoken on the wire, held in the
/// topology cache, and projected by HTTP surfaces. Sections, not a flat
/// field zoo. (No Default: a frame without a speaking stone is meaningless.)
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
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
