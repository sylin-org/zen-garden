//! [`IdentityProtocol`] — the pluggable layer that parses an opened
//! device into an [`Identification`].
//!
//! An identity protocol is registered by a consumer crate (today:
//! firefly; tomorrow: a future Bluetooth Pico, a network-advertised
//! service, …). The bus invokes identity protocols sequentially when
//! a new device attaches; the first one to return `Ok(Some(id))`
//! wins and its descriptor flows downstream.
//!
//! A protocol returns:
//!
//! - `Ok(Some(id))` — "this is mine, here's the parsed descriptor."
//! - `Ok(None)` — "not mine; try the next protocol."
//! - `Err(_)` — "this device tried to respond but the response was
//!   malformed." Bus records the error and marks the port for backoff.

use super::descriptor::Identification;
use super::device::OpenedDevice;

/// Bridges an opened device to a structured identification.
pub trait IdentityProtocol: Send + Sync {
    /// The ecosystem marker this protocol stamps onto identifications
    /// it produces.
    fn ecosystem(&self) -> &'static str;

    /// Attempt to identify the device.
    fn identify(&self, device: &mut OpenedDevice) -> IdentifyResult;
}

/// Outcome of an identity probe.
pub type IdentifyResult = Result<Option<Identification>, IdentifyError>;

/// Error from an identity probe. The bus marks the port for backoff
/// on either variant; the difference drives telemetry shape.
#[derive(Debug, thiserror::Error)]
pub enum IdentifyError {
    /// The device answered but the response could not be parsed.
    /// Examples: JSON syntax error, missing required fields.
    #[error("malformed identification: {0}")]
    Malformed(String),

    /// I/O error talking to the device (timeout, disconnect, EOF).
    #[error("i/o error during identification: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::super::device::Device;
    use super::*;
    use serde_json::json;

    /// A stub identity protocol that matches any mock device whose
    /// buffer parses as JSON carrying `ecosystem == self.ecosystem()`.
    struct StubProtocol {
        eco: &'static str,
    }

    impl IdentityProtocol for StubProtocol {
        fn ecosystem(&self) -> &'static str {
            self.eco
        }

        fn identify(&self, device: &mut OpenedDevice) -> IdentifyResult {
            let buf = device
                .mock_buffer()
                .ok_or_else(|| IdentifyError::Io("not a mock device".into()))?;
            let bytes = buf.lock().unwrap().clone();
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| IdentifyError::Malformed(e.to_string()))?;
            if value.get("ecosystem").and_then(|v| v.as_str()) != Some(self.eco) {
                return Ok(None);
            }
            Ok(Identification::from_json(self.eco, value))
        }
    }

    fn fake_device() -> Device {
        Device::usb_serial("test", 0x1a86, 0x7523, None, "/dev/ttyMOCK0")
    }

    #[test]
    fn stub_protocol_accepts_matching_ecosystem() {
        let proto = StubProtocol { eco: "firefly" };
        let payload = json!({
            "ecosystem": "firefly",
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
        })
        .to_string()
        .into_bytes();
        let mut opened = OpenedDevice::mock(fake_device(), payload);
        let id = proto.identify(&mut opened).unwrap().unwrap();
        assert_eq!(id.ecosystem, "firefly");
        assert_eq!(id.device_id, "01938abc-de01-7234-89ab-cdef01234567");
    }

    #[test]
    fn stub_protocol_passes_on_other_ecosystem() {
        let proto = StubProtocol { eco: "firefly" };
        let payload = json!({
            "ecosystem": "other",
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
        })
        .to_string()
        .into_bytes();
        let mut opened = OpenedDevice::mock(fake_device(), payload);
        assert!(proto.identify(&mut opened).unwrap().is_none());
    }

    #[test]
    fn stub_protocol_reports_malformed_on_bad_json() {
        let proto = StubProtocol { eco: "firefly" };
        let mut opened = OpenedDevice::mock(fake_device(), b"not-json".to_vec());
        let err = proto.identify(&mut opened).unwrap_err();
        matches!(err, IdentifyError::Malformed(_));
    }
}
