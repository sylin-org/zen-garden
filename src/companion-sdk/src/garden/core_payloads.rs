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
}
