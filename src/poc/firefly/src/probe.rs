//! [`FireflyProbe`] — evaluates a candidate [`UsbSerialDevice`].
//!
//! The *only* code that writes `I\n` and parses the response at the
//! firefly boundary. Given a device in `Evaluating` state, returns
//! either an `Arc<Firefly>` (probe succeeded, device is a firefly)
//! or a reason string (not a firefly / firmware silent / malformed).

use crate::firefly::{Firefly, Identity};
use anyhow::Result;
use garden_companion_usb::UsbSerialDevice;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::broadcast;

/// Deadline for receiving a valid identity line after the trigger write.
const READ_DEADLINE: Duration = Duration::from_secs(4);

const _: () = assert!(
    READ_DEADLINE.as_secs() == 4,
    "READ_DEADLINE changed — review firefly handshake latency assumptions \
     (ESP boot: ~2.5s; identity emit: ~200ms; budget for USB hiccups: ~1.3s)"
);

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("device did not respond within deadline")]
    Timeout,
    #[error("legacy firefly firmware (pre-JSON identity) — re-run NewFirefly to update the board")]
    LegacyFirmware,
    #[error("i/o error: {0}")]
    Io(String),
    #[error("line stream closed")]
    StreamClosed,
}

pub struct FireflyProbe;

impl FireflyProbe {
    /// Evaluate a candidate device. The device must be in the
    /// `Evaluating` state; the orchestrator transitions before
    /// calling.
    pub async fn evaluate(device: Arc<UsbSerialDevice>) -> Result<Arc<Firefly>, ProbeError> {
        let started = Instant::now();
        let mut rx = device.lines();
        tracing::debug!(device = %device.id(), "probe: subscribed to lines");

        device
            .send(b"I\n")
            .await
            .map_err(|e| ProbeError::Io(e.to_string()))?;
        tracing::debug!(device = %device.id(), "probe: sent I\\n");

        let deadline = started + READ_DEADLINE;
        let mut lines_seen = 0u32;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                tracing::debug!(
                    device = %device.id(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    lines_seen,
                    "probe: timeout"
                );
                return Err(ProbeError::Timeout);
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(line)) => {
                    lines_seen += 1;
                    tracing::debug!(
                        device = %device.id(),
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        line = %line,
                        "probe: line"
                    );
                    if let Some(identity) = try_parse_identity(&line) {
                        tracing::debug!(
                            device = %device.id(),
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            lines_seen,
                            "probe: identity parsed"
                        );
                        return Ok(Firefly::new(device, identity));
                    }
                    if is_legacy_identity(&line) {
                        tracing::debug!(
                            device = %device.id(),
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            line = %line,
                            "probe: legacy (pre-JSON) firefly identity"
                        );
                        return Err(ProbeError::LegacyFirmware);
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                    tracing::debug!(device = %device.id(), skipped = n, "probe: broadcast lagged");
                    continue;
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    tracing::debug!(device = %device.id(), "probe: stream closed");
                    return Err(ProbeError::StreamClosed);
                }
                Err(_) => {
                    tracing::debug!(
                        device = %device.id(),
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        lines_seen,
                        "probe: tokio timeout"
                    );
                    return Err(ProbeError::Timeout);
                }
            }
        }
    }
}

fn try_parse_identity(line: &str) -> Option<Identity> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let json_str = line.strip_prefix("OK,").unwrap_or(line);
    let json_str = json_str.strip_prefix("* HELLO,").unwrap_or(json_str);
    if !json_str.trim_start().starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(json_str).ok()?;
    Identity::parse(value).ok()
}

/// Detect a legacy (pre-JSON) firefly identity reply. Old firmware answers `I`
/// with a CSV line like `OK,firefly-oled,esp8266,...` rather than JSON. Recognising
/// it lets the probe report a clear "update the firmware" reason instead of a
/// misleading timeout — the board *is* a firefly, just on the old protocol.
fn is_legacy_identity(line: &str) -> bool {
    let line = line.trim();
    let rest = line.strip_prefix("OK,").unwrap_or(line);
    let rest = rest.strip_prefix("* HELLO,").unwrap_or(rest);
    let rest = rest.trim_start();
    rest.starts_with("firefly-") || rest.starts_with("firefly,")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_ok_prefixed_json() {
        let line = format!(
            "OK,{}",
            json!({
                "family": "firefly",
                "variant": "oled",
                "device_id": "abc",
                "capabilities": ["dashboard"]
            })
        );
        let id = try_parse_identity(&line).unwrap();
        assert_eq!(id.variant, "oled");
    }

    #[test]
    fn parses_hello_frame() {
        let line = r#"* HELLO,{"family":"firefly","variant":"tdisplay","device_id":"x"}"#;
        let id = try_parse_identity(line).unwrap();
        assert_eq!(id.variant, "tdisplay");
    }

    #[test]
    fn rejects_non_firefly() {
        let line = r#"OK,{"family":"cricket","variant":"audio"}"#;
        assert!(try_parse_identity(line).is_none());
    }

    #[test]
    fn rejects_boot_noise() {
        assert!(try_parse_identity("booting...").is_none());
        assert!(try_parse_identity("OK,ready").is_none());
        assert!(try_parse_identity("").is_none());
    }

    #[test]
    fn detects_legacy_csv_identity() {
        assert!(is_legacy_identity(
            "OK,firefly-oled,esp8266,128x64,dual-zone:yellow:16:blue:48,v0.2.0"
        ));
        assert!(is_legacy_identity("OK,firefly-oled-v2,esp8266,128x64"));
        assert!(is_legacy_identity("* HELLO,firefly-matrix,rp2040"));
        // a legacy line is not a valid (JSON) identity — so the probe would
        // otherwise loop to a misleading timeout
        assert!(try_parse_identity("OK,firefly-oled,esp8266,128x64,v0.2.0").is_none());
    }

    #[test]
    fn legacy_detector_ignores_json_and_noise() {
        assert!(!is_legacy_identity(r#"OK,{"family":"firefly","variant":"oled"}"#));
        assert!(!is_legacy_identity("booting..."));
        assert!(!is_legacy_identity("OK,ready"));
        assert!(!is_legacy_identity(""));
    }
}
