//! The chirp body — a stone's presence, spoken every heartbeat and on change.
//!
//! Field-by-field redesign of the PoC's `TopologyEntry` chirp (COMM-0005
//! audit applied): the v0-*required* core is kept byte-compatible so v0
//! stones parse v1 chirps; v0-optional fields that receivers overwrite
//! anyway (`discovered_at`, `last_seen`, `status`) are still emitted for
//! that reason; heavy hardware detail is deliberately *absent* — it moves
//! to the v1-only detail beacon (DEBT D4), shrinking the periodic wire.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// How to reach a stone's HTTP surface. Field-compatible with the PoC's
/// `PeerAddress`.
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

/// One offering a stone runs, as seen in presence. Field-compatible with
/// the PoC's `TopologyServiceEntry` (its `name` is a string on the wire).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceEntry {
    /// Stable identity of the offering instance; survives renames.
    #[serde(default)]
    pub offering_id: String,
    /// Fully-qualified offering name (e.g. `mongodb` or `mongodb::legacy`).
    pub name: String,
    /// Catalog offering this instance was planted from.
    pub offering: String,
    /// Catalog category (glossary noun).
    pub category: String,
    /// Runtime status (running / stopped / degraded).
    pub status: String,
    /// Orchestration role when active: primary | replica | joining | degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// The chirp body. v0-required core + v1 extensions (`proto`, `boot_id`,
/// `seq`) that v0 stones ignore. Hardware capabilities intentionally absent
/// — they travel on the detail beacon (DEBT D4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChirpBody {
    /// Persistent stone identity (GUIDv7 in the PoC; v1 keeps the shape).
    pub stone_id: String,
    /// Human-facing name; also the mDNS label.
    pub stone_name: String,
    /// Where the HTTP surface lives.
    pub address: PeerAddress,
    /// Software version, self-reported.
    pub moss_version: String,
    /// Offerings this stone runs.
    pub services: Vec<ServiceEntry>,
    /// Self-assessed vitality (glossary::health).
    pub health: String,
    /// Membership state as spoken (peers track their own view); v0 requires
    /// the field, so v1 always emits `online` while chirping.
    pub status: String,
    /// First-seen timestamp (v0 requires; peers overwrite with their own).
    pub discovered_at: DateTime<Utc>,
    /// Last-seen timestamp (v0 requires; peers overwrite with their own).
    pub last_seen: DateTime<Utc>,
    /// MAC for wake-on-LAN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,

    // ---- v1 extensions: unknown fields to v0 parsers, silently ignored ----
    /// Wire schema marker; [`crate::consts::PROTO_V1`] when spoken by v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto: Option<String>,
    /// Identity of this boot — lets peers distinguish restart from heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
    /// Monotonic chirp counter for this boot — gap detection, ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}
