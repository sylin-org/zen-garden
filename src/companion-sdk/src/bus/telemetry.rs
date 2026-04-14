//! Telemetry event payloads the bus emits into `Pulse` on
//! discovery-path failures.
//!
//! Three distinct failure modes, three distinct event kinds — so
//! operators can grep telemetry for the specific shape of their
//! problem rather than one opaque "device won't work" signal.

use crate::garden::EventPayload;
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Emitted when an identity protocol parsed a descriptor but the
/// descriptor lacks a `device_id` — device is running firmware but
/// was never through `newfirefly.ps1` (or equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceUnprovisioned {
    pub port: String,
    pub ecosystem: String,
    pub raw_descriptor: serde_json::Value,
}

impl EventPayload for DeviceUnprovisioned {
    const KIND: &'static str = "core.companion.device.unprovisioned";
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Emitted when the descriptor was well-formed and provisioned but no
/// adapter registration matched. Indicates daemon too old / firmware
/// too new / capability mismatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceUnclaimed {
    pub port: String,
    pub device_id: String,
    pub descriptor: serde_json::Value,
}

impl EventPayload for DeviceUnclaimed {
    const KIND: &'static str = "core.companion.device.unclaimed";
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Emitted when every registered identity protocol returned `None` —
/// device didn't speak any protocol we know. Random USB gadget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceForeign {
    pub port: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub product: Option<String>,
}

impl EventPayload for DeviceForeign {
    const KIND: &'static str = "core.companion.device.foreign";
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_kinds_are_stable_strings() {
        assert_eq!(
            DeviceUnprovisioned::KIND,
            "core.companion.device.unprovisioned"
        );
        assert_eq!(DeviceUnclaimed::KIND, "core.companion.device.unclaimed");
        assert_eq!(DeviceForeign::KIND, "core.companion.device.foreign");
    }

    #[test]
    fn payloads_round_trip_through_serde() {
        let u = DeviceUnprovisioned {
            port: "/dev/ttyUSB0".into(),
            ecosystem: "firefly".into(),
            raw_descriptor: serde_json::json!({"family": "firefly"}),
        };
        let j = serde_json::to_string(&u).unwrap();
        let back: DeviceUnprovisioned = serde_json::from_str(&j).unwrap();
        assert_eq!(back.port, u.port);
        assert_eq!(back.ecosystem, u.ecosystem);
    }
}
