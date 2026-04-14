//! Core event payloads.
//!
//! Implements [`EventPayload`] for the typed payload structs used by the
//! SDK. Payloads pulled from `garden-common::presence` (pre-existing wire
//! types) get their `EventPayload` impls here; payloads that don't yet
//! have a typed wire struct get defined here as companion-sdk types with
//! deserialization matching the moss wire format.
//!
//! Each payload assigns the canonical `core.*` kind. The translation from
//! wire kind (e.g. `stone.load.updated`) to canonical kind (e.g.
//! `core.stone.load.updated`) happens at the SseTransport boundary via
//! [`wire_to_core_kind`].
//!
//! Unknown wire kinds are logged at `warn` level and skipped. Promoting a
//! new wire kind to a typed payload is a local change: (1) add a struct
//! here, (2) add an `EventPayload` impl, (3) extend [`WIRE_KIND_MAP`].
//!
//! [`EventPayload`]: crate::garden::EventPayload
//! [`wire_to_core_kind`]: self::wire_to_core_kind

use super::event::EventPayload;
use garden_common::domain::{Health, Load, Pond, SeedBank};
use garden_common::presence::{PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload};
use serde::{Deserialize, Serialize};
use std::any::Any;

// ---------------------------------------------------------------------------
// Canonical kind constants
// ---------------------------------------------------------------------------

pub const KIND_PRESENCE_SNAPSHOT: &str = "core.presence.snapshot";
pub const KIND_STONE_HEALTH_CHANGED: &str = "core.stone.health.changed";
pub const KIND_STONE_LOAD_UPDATED: &str = "core.stone.load.updated";
pub const KIND_STONE_TENDED: &str = "core.stone.tended";
pub const KIND_SERVICE_STARTED: &str = "core.service.started";
pub const KIND_SERVICE_STOPPED: &str = "core.service.stopped";
pub const KIND_STORAGE_CONNECTED: &str = "core.storage.connected";
pub const KIND_STORAGE_DETECTED: &str = "core.storage.detected";
pub const KIND_STORAGE_REMOVED: &str = "core.storage.removed";

// ---------------------------------------------------------------------------
// EventPayload impls on shared garden-common wire types
// ---------------------------------------------------------------------------

impl EventPayload for PresenceSnapshot {
    const KIND: &'static str = KIND_PRESENCE_SNAPSHOT;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl EventPayload for StoneHealthChangedPayload {
    const KIND: &'static str = KIND_STONE_HEALTH_CHANGED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl EventPayload for StoneLoadUpdatedPayload {
    const KIND: &'static str = KIND_STONE_LOAD_UPDATED;
    const COALESCING: bool = true;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// SDK-local typed payloads (moss wraps each event's SSE data with common
// `timestamp` + `message` fields; we accept them via #[serde(default)] +
// ignore-unknown semantics but only expose the domain-relevant fields).
// ---------------------------------------------------------------------------

/// Payload for `core.stone.tended` events. Someone (operator, rake, UI)
/// acknowledged this stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneTendedPayload {
    /// Who tended (e.g. `"rake"`, `"dashboard"`).
    #[serde(default)]
    pub by: String,

    /// Where they tended from (hostname, IP, etc.).
    #[serde(default)]
    pub from: String,

    /// Optional human-readable message.
    #[serde(default)]
    pub message: Option<String>,
}

impl EventPayload for StoneTendedPayload {
    const KIND: &'static str = KIND_STONE_TENDED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Payload for `core.service.started` events. An offering has transitioned
/// to a running state on this stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStartedPayload {
    /// Offering name (the `service` field in moss's presence format).
    #[serde(default)]
    pub service: String,
}

impl EventPayload for ServiceStartedPayload {
    const KIND: &'static str = KIND_SERVICE_STARTED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Payload for `core.service.stopped` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStoppedPayload {
    #[serde(default)]
    pub service: String,
}

impl EventPayload for ServiceStoppedPayload {
    const KIND: &'static str = KIND_SERVICE_STOPPED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Payload for `core.storage.connected` events. A managed seed-bank was
/// adopted (is present with `.zen-garden/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConnectedPayload {
    /// Seed-bank name.
    #[serde(default)]
    pub name: String,

    /// Capacity in gigabytes (0 if unknown).
    #[serde(default)]
    pub capacity_gb: u64,

    /// Block device path (e.g. `/dev/sdb1`).
    #[serde(default)]
    pub device: Option<String>,

    /// Mount path (e.g. `/mnt/seed-bank-main`).
    #[serde(default)]
    pub mount_path: Option<String>,
}

impl EventPayload for StorageConnectedPayload {
    const KIND: &'static str = KIND_STORAGE_CONNECTED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Payload for `core.storage.detected` events. Unmanaged storage appeared.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDetectedPayload {
    #[serde(default)]
    pub device: Option<String>,
}

impl EventPayload for StorageDetectedPayload {
    const KIND: &'static str = KIND_STORAGE_DETECTED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Payload for `core.storage.removed` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRemovedPayload {
    #[serde(default)]
    pub name: String,
}

impl EventPayload for StorageRemovedPayload {
    const KIND: &'static str = KIND_STORAGE_REMOVED;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Wire-kind translation
// ---------------------------------------------------------------------------

/// Translate a wire kind (as emitted by moss) to our canonical `core.*`
/// kind, returning `None` if the kind is not in [`WIRE_KIND_MAP`].
///
/// SseTransport calls this on every incoming SSE frame. Unknown kinds
/// are log-warn-and-skipped.
pub fn wire_to_core_kind(wire_kind: &str) -> Option<&'static str> {
    WIRE_KIND_MAP
        .iter()
        .find_map(|(wire, core)| (wire_kind == *wire).then_some(*core))
}

/// Wire kind → canonical core kind. Extend this table to support a new
/// moss event type.
pub(crate) const WIRE_KIND_MAP: &[(&str, &str)] = &[
    (
        garden_common::presence::event_types::PRESENCE_SNAPSHOT,
        KIND_PRESENCE_SNAPSHOT,
    ),
    (
        garden_common::presence::event_types::STONE_HEALTH_CHANGED,
        KIND_STONE_HEALTH_CHANGED,
    ),
    (
        garden_common::presence::event_types::STONE_LOAD_UPDATED,
        KIND_STONE_LOAD_UPDATED,
    ),
    (
        garden_common::presence::event_types::STONE_TENDED,
        KIND_STONE_TENDED,
    ),
    (
        garden_common::presence::event_types::SERVICE_STARTED,
        KIND_SERVICE_STARTED,
    ),
    (
        garden_common::presence::event_types::SERVICE_STOPPED,
        KIND_SERVICE_STOPPED,
    ),
    (
        garden_common::presence::event_types::STORAGE_CONNECTED,
        KIND_STORAGE_CONNECTED,
    ),
    (
        garden_common::presence::event_types::STORAGE_DETECTED,
        KIND_STORAGE_DETECTED,
    ),
    (
        garden_common::presence::event_types::STORAGE_REMOVED,
        KIND_STORAGE_REMOVED,
    ),
];

// ---------------------------------------------------------------------------
// Typed domain accessors — added in COMPANION-0005 (Book IV)
// ---------------------------------------------------------------------------

/// Typed-domain extension methods on [`StoneHealthChangedPayload`].
pub trait StoneHealthChangedExt {
    /// Parse the wire `health` string into a typed [`Health`] value.
    ///
    /// Returns [`Health::Dormant`] for unrecognized values, so callers that
    /// don't care about the distinction get sensible default behaviour
    /// (treating unknown as offline).
    fn health_domain(&self) -> Health;
}

impl StoneHealthChangedExt for StoneHealthChangedPayload {
    fn health_domain(&self) -> Health {
        Health::parse(&self.health).unwrap_or(Health::Dormant)
    }
}

/// Typed-domain extension methods on [`StoneLoadUpdatedPayload`].
pub trait StoneLoadUpdatedExt {
    /// Typed snapshot of this load event.
    fn load_domain(&self) -> Load;
}

impl StoneLoadUpdatedExt for StoneLoadUpdatedPayload {
    fn load_domain(&self) -> Load {
        Load::from(self)
    }
}

/// Typed-domain extension methods on [`PresenceSnapshot`].
pub trait PresenceSnapshotExt {
    /// Stone's health, parsed from the wire string. Falls back to
    /// [`Health::Dormant`] for unrecognized values.
    fn stone_health(&self) -> Health;

    /// Stone's resource load as a cohesive [`Load`] value.
    fn stone_load(&self) -> Load;

    /// Stone's seed-bank summary, if any is attached.
    fn seed_bank(&self) -> Option<SeedBank>;

    /// Stone's pond membership state (best-effort — wire shape only
    /// carries a boolean, so we cannot distinguish Member from
    /// Cornerstone here).
    fn pond(&self) -> Pond;
}

impl PresenceSnapshotExt for PresenceSnapshot {
    fn stone_health(&self) -> Health {
        Health::parse(&self.stone.health).unwrap_or(Health::Dormant)
    }

    fn stone_load(&self) -> Load {
        Load {
            cpu: garden_common::domain::Percent::new(self.stone.cpu_percent),
            memory: garden_common::domain::Percent::new(self.stone.memory_percent),
            disk: garden_common::domain::Percent::new(self.stone.disk_percent),
            io: garden_common::domain::Percent::new(self.stone.io_percent),
            gpu: garden_common::domain::Percent::new(self.stone.gpu_percent),
            gpu_active: self.stone.gpu_active,
            net_rx_bytes_per_sec: self.stone.net_rx_bytes_per_sec,
            net_tx_bytes_per_sec: self.stone.net_tx_bytes_per_sec,
        }
    }

    fn seed_bank(&self) -> Option<SeedBank> {
        self.stone.seed_bank.as_ref().map(SeedBank::from)
    }

    fn pond(&self) -> Pond {
        Pond::from_active_flag(self.stone.pond_active)
    }
}

// ---------------------------------------------------------------------------
// SSE transport kind catalog
// ---------------------------------------------------------------------------

/// All canonical kinds SseTransport may emit. Used by
/// [`Transport::emitted_kinds`] so Companion can register the namespaces.
///
/// [`Transport::emitted_kinds`]: crate::garden::Transport::emitted_kinds
pub(crate) const SSE_EMITTED_KINDS: &[&str] = &[
    KIND_PRESENCE_SNAPSHOT,
    KIND_STONE_HEALTH_CHANGED,
    KIND_STONE_LOAD_UPDATED,
    KIND_STONE_TENDED,
    KIND_SERVICE_STARTED,
    KIND_SERVICE_STOPPED,
    KIND_STORAGE_CONNECTED,
    KIND_STORAGE_DETECTED,
    KIND_STORAGE_REMOVED,
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{Event, is_valid_kind};
    use garden_common::presence::StoneState;

    fn fresh_snapshot() -> PresenceSnapshot {
        PresenceSnapshot {
            stone: StoneState {
                name: "test-stone".into(),
                health: "thriving".into(),
                cpu_percent: 0.0,
                memory_percent: 0.0,
                disk_percent: 0.0,
                uptime_seconds: 0,
                pond_active: false,
                io_percent: 0.0,
                gpu_percent: 0.0,
                net_rx_bytes_per_sec: 0,
                net_tx_bytes_per_sec: 0,
                has_gpu: false,
                gpu_active: false,
                is_lantern: false,
                has_cricket: false,
                hour: 0.0,
                seed_bank: None,
            },
            offerings: vec![],
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn presence_snapshot_kind_matches_core_namespace() {
        assert_eq!(PresenceSnapshot::KIND, "core.presence.snapshot");
        assert!(is_valid_kind(PresenceSnapshot::KIND));
    }

    #[test]
    fn stone_load_updated_is_coalescing_and_non_load_is_not() {
        assert!(StoneLoadUpdatedPayload::COALESCING);
        assert!(!StoneHealthChangedPayload::COALESCING);
        assert!(!StoneTendedPayload::COALESCING);
        assert!(!ServiceStartedPayload::COALESCING);
        assert!(!ServiceStoppedPayload::COALESCING);
        assert!(!StorageConnectedPayload::COALESCING);
        assert!(!StorageDetectedPayload::COALESCING);
        assert!(!StorageRemovedPayload::COALESCING);
    }

    #[test]
    fn snapshot_round_trips_through_envelope() {
        let snap = fresh_snapshot();
        let evt = Event::new(snap);
        assert_eq!(evt.kind, "core.presence.snapshot");
        let recovered = evt.payload::<PresenceSnapshot>().unwrap();
        assert_eq!(recovered.stone.name, "test-stone");
    }

    #[test]
    fn tended_payload_deserializes_from_moss_wire_shape() {
        // Moss's pulse listener builds data as:
        //   { "by": "...", "from": "...", "message": "..." }
        // plus the presence listener layers on "timestamp" and "message"
        // at the top level. We need to tolerate both.
        let wire = r#"{
            "timestamp": "2026-04-13T12:00:00Z",
            "message": "Stone tended",
            "by": "rake",
            "from": "leo-laptop"
        }"#;
        let payload: StoneTendedPayload = serde_json::from_str(wire).unwrap();
        assert_eq!(payload.by, "rake");
        assert_eq!(payload.from, "leo-laptop");
    }

    #[test]
    fn service_started_payload_deserializes() {
        let wire = r#"{ "timestamp": "...", "message": "Started", "service": "mongodb" }"#;
        let payload: ServiceStartedPayload = serde_json::from_str(wire).unwrap();
        assert_eq!(payload.service, "mongodb");
    }

    #[test]
    fn storage_connected_payload_deserializes() {
        let wire = r#"{
            "name": "backup",
            "capacity_gb": 500,
            "device": "/dev/sdb1",
            "mount_path": "/mnt/backup"
        }"#;
        let payload: StorageConnectedPayload = serde_json::from_str(wire).unwrap();
        assert_eq!(payload.name, "backup");
        assert_eq!(payload.capacity_gb, 500);
        assert_eq!(payload.device.as_deref(), Some("/dev/sdb1"));
    }

    #[test]
    fn wire_to_core_translates_known_kinds() {
        assert_eq!(
            wire_to_core_kind("stone.load.updated"),
            Some("core.stone.load.updated")
        );
        assert_eq!(
            wire_to_core_kind("presence.snapshot"),
            Some("core.presence.snapshot")
        );
        assert_eq!(
            wire_to_core_kind("storage.connected"),
            Some("core.storage.connected")
        );
        assert_eq!(
            wire_to_core_kind("stone.tended"),
            Some("core.stone.tended")
        );
    }

    #[test]
    fn wire_to_core_returns_none_for_unknown() {
        assert_eq!(wire_to_core_kind("unknown.event.kind"), None);
        assert_eq!(wire_to_core_kind(""), None);
    }

    #[test]
    fn all_wire_map_entries_produce_valid_kinds() {
        for (_wire, core) in WIRE_KIND_MAP {
            assert!(
                is_valid_kind(core),
                "wire→core entry produced invalid kind: {}",
                core
            );
            assert!(
                core.starts_with("core."),
                "wire→core entry not in core.* namespace: {}",
                core
            );
        }
    }

    #[test]
    fn sse_emitted_kinds_matches_wire_map_targets() {
        let map_cores: std::collections::HashSet<&str> =
            WIRE_KIND_MAP.iter().map(|(_, c)| *c).collect();
        let emitted: std::collections::HashSet<&str> = SSE_EMITTED_KINDS.iter().copied().collect();
        assert_eq!(map_cores, emitted);
    }

    // --- Typed domain accessors (COMPANION-0005 Book IV) ---

    #[test]
    fn health_domain_parses_known_wire_string() {
        let payload = StoneHealthChangedPayload {
            health: "thriving".into(),
            cpu_percent: 10.0,
            memory_percent: 20.0,
        };
        assert_eq!(payload.health_domain(), Health::Thriving);
    }

    #[test]
    fn health_domain_defaults_to_dormant_for_unknown() {
        let payload = StoneHealthChangedPayload {
            health: "something-unrecognized".into(),
            cpu_percent: 0.0,
            memory_percent: 0.0,
        };
        assert_eq!(payload.health_domain(), Health::Dormant);
    }

    #[test]
    fn load_domain_builds_typed_snapshot() {
        let payload = StoneLoadUpdatedPayload {
            cpu_percent: 42.0,
            memory_percent: 55.0,
            disk_percent: 30.0,
            io_percent: 12.0,
            gpu_percent: 80.0,
            gpu_active: true,
            net_rx_bytes_per_sec: 1_000,
            net_tx_bytes_per_sec: 500,
        };
        let load = payload.load_domain();
        assert_eq!(load.cpu.value(), 42.0);
        assert_eq!(load.memory.value(), 55.0);
        assert_eq!(load.net_total_bytes_per_sec(), 1_500);
        assert!(load.gpu_active);
    }

    #[test]
    fn snapshot_typed_accessors_reflect_stone_state() {
        let mut snap = fresh_snapshot();
        snap.stone.health = "wilting".into();
        snap.stone.cpu_percent = 95.5;
        snap.stone.pond_active = true;

        assert_eq!(snap.stone_health(), Health::Wilting);
        assert_eq!(snap.stone_load().cpu.as_u8(), 96);
        assert_eq!(snap.pond(), garden_common::domain::Pond::Member);
        assert!(snap.seed_bank().is_none());
    }

    #[test]
    fn snapshot_seed_bank_accessor_passes_through_when_present() {
        let mut snap = fresh_snapshot();
        snap.stone.seed_bank = Some(garden_common::presence::StoragePresence {
            name: "primary".into(),
            used_gb: 100,
            total_gb: 500,
        });

        let bank = snap.seed_bank().unwrap();
        assert_eq!(bank.name, "primary");
        assert_eq!(bank.used_gb, 100);
        assert_eq!(bank.free_gb(), 400);
    }
}
