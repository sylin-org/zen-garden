//! [`Identification`] — a structured descriptor parsed by an identity
//! protocol from an opened device.
//!
//! `Identification` is the bridge between "an unknown thing on a
//! serial port" and "an adapter predicate match." Carries:
//!
//! - `ecosystem` — stamped by the identity protocol that parsed it;
//!   provenance marker (e.g. `"firefly"`).
//! - `device_id` — the stable, per-device GUIDv7 minted by the
//!   provisioning ritual and embedded in firmware. Bus uses this as
//!   the identity-cache key.
//! - `hardware_id` — chip-unique id for forensic diagnostics.
//! - `fields` — the full parsed JSON descriptor. Adapter registration
//!   predicates evaluate against this. Schema lives in the ecosystem's
//!   own ADR (see FIREFLY-0004 for firefly).

use serde::{Deserialize, Serialize};

/// Structured identification parsed from a device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identification {
    /// The identity-protocol-stamped ecosystem marker. Immutable once
    /// stamped; adapters filter on this first.
    pub ecosystem: String,

    /// GUIDv7 (or other stable string id) identifying *this* physical
    /// device across reboots, port renumberings, and host changes.
    pub device_id: String,

    /// Chip-unique forensic id. `None` when the ecosystem doesn't
    /// surface one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_id: Option<String>,

    /// Full descriptor as emitted by the device. Adapter predicates
    /// evaluate against this. Unknown fields are preserved for
    /// forward-compat.
    pub fields: serde_json::Value,
}

impl Identification {
    /// Build an `Identification` from a parsed JSON object and an
    /// ecosystem stamp. Extracts required top-level fields.
    ///
    /// Returns `None` when the object is missing `device_id` — that's
    /// the "unprovisioned" signal; the bus treats it distinctly from
    /// "parseable but no match."
    pub fn from_json(ecosystem: impl Into<String>, value: serde_json::Value) -> Option<Self> {
        let device_id = value.get("device_id")?.as_str()?.to_string();
        let hardware_id = value
            .get("hardware_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        Some(Self {
            ecosystem: ecosystem.into(),
            device_id,
            hardware_id,
            fields: value,
        })
    }

    /// Read a string field from the descriptor, returning `None` when
    /// absent or not a string.
    pub fn string_field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(|v| v.as_str())
    }

    /// `true` when the descriptor advertises the given capability in
    /// its `capabilities` array (of strings).
    pub fn has_capability(&self, name: &str) -> bool {
        self.fields
            .get("capabilities")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|c| c.as_str() == Some(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_json_extracts_device_id_and_hardware_id() {
        let value = json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "hardware_id": "esp32-3c:71:bf:25:ab:90",
            "family": "firefly",
            "variant": "tdisplay",
        });
        let id = Identification::from_json("firefly", value).unwrap();
        assert_eq!(id.ecosystem, "firefly");
        assert_eq!(id.device_id, "01938abc-de01-7234-89ab-cdef01234567");
        assert_eq!(
            id.hardware_id.as_deref(),
            Some("esp32-3c:71:bf:25:ab:90")
        );
        assert_eq!(id.string_field("variant"), Some("tdisplay"));
    }

    #[test]
    fn from_json_returns_none_without_device_id() {
        let value = json!({
            "family": "firefly",
            "variant": "oled",
        });
        assert!(Identification::from_json("firefly", value).is_none());
    }

    #[test]
    fn has_capability_checks_array_entries() {
        let value = json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "capabilities": ["dashboard", "brightness"],
        });
        let id = Identification::from_json("firefly", value).unwrap();
        assert!(id.has_capability("dashboard"));
        assert!(id.has_capability("brightness"));
        assert!(!id.has_capability("gpu-bar"));
    }

    #[test]
    fn has_capability_absent_array_returns_false() {
        let value = json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
        });
        let id = Identification::from_json("firefly", value).unwrap();
        assert!(!id.has_capability("anything"));
    }

    #[test]
    fn identification_roundtrips_through_serde() {
        let value = json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
        });
        let id = Identification::from_json("firefly", value).unwrap();
        let encoded = serde_json::to_string(&id).unwrap();
        let decoded: Identification = serde_json::from_str(&encoded).unwrap();
        assert_eq!(id, decoded);
    }
}
