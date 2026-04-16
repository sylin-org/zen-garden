//! [`FireflyProbe`] — evaluates a candidate [`UsbSerialDevice`].
//!
//! The *only* code that writes `I\n` and parses the response at the
//! firefly boundary. Given a device in `Evaluating` state, returns
//! either an `Arc<Firefly>` (probe succeeded, device is a firefly)
//! or a reason string (not a firefly / firmware silent / malformed).

use crate::firefly::{Firefly, Identity};
use anyhow::Result;
use garden_companion_sdk::usb_devices::UsbSerialDevice;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::broadcast;

/// Deadline for receiving a valid identity line after the trigger write.
const READ_DEADLINE: Duration = Duration::from_secs(4);

const fn _deadline_is_four_secs() {
    // Anchored to observe drift; kept here so a future edit surfaces
    // in review.
}

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("device did not respond within deadline")]
    Timeout,
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
}
