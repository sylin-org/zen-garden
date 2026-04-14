//! Firefly identity protocol — bridges an opened USB serial port to
//! a FIREFLY-0004 descriptor.
//!
//! Listens for up to ~3 s for an unsolicited `* HELLO,<json>\n`
//! frame. If none arrives, sends `I\n` and reads one `OK,<json>\n`
//! reply. Parses the JSON body, stamps the `"firefly"` ecosystem,
//! and returns an [`Identification`].
//!
//! Any device that doesn't answer either frame flavour is `None` —
//! the bus moves on to the next protocol (or classifies the port as
//! foreign if none match).

use garden_companion_sdk::bus::{
    IdentifyError, IdentifyResult, Identification, IdentityProtocol, OpenedDevice,
};
use std::time::{Duration, Instant};

/// Total time the identity probe waits for an identification frame.
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);

/// Per-byte read timeout inside the main loop — small so the outer
/// deadline fires promptly when the device is silent.
const INNER_TIMEOUT: Duration = Duration::from_millis(200);

/// Maximum descriptor frame size. Safety bound so a runaway device
/// can't exhaust memory with an unterminated line.
const MAX_LINE_LEN: usize = 4096;

pub struct FireflyIdentityProtocol;

impl FireflyIdentityProtocol {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for FireflyIdentityProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityProtocol for FireflyIdentityProtocol {
    fn ecosystem(&self) -> &'static str {
        "firefly"
    }

    fn identify(&self, device: &mut OpenedDevice) -> IdentifyResult {
        // Mock devices (integration tests) expose a byte buffer; parse
        // it as-is. Real devices go through the serial path below.
        if let Some(buffer) = device.mock_buffer() {
            let bytes = buffer.lock().unwrap().clone();
            let json = parse_identification_frame(&bytes)?;
            return Ok(Identification::from_json("firefly", json));
        }

        let Some(port) = device.as_usb_serial() else {
            return Ok(None);
        };

        // Phase 1: wait for an unsolicited HELLO.
        let mut buf = Vec::with_capacity(512);
        let deadline = Instant::now() + HELLO_TIMEOUT;
        {
            let mut guard = port.lock().map_err(|e| {
                IdentifyError::Io(format!("serial lock poisoned: {e}"))
            })?;
            let _ = guard.set_timeout(INNER_TIMEOUT);
            if let Some(json) = read_frame_until(&mut **guard, &mut buf, deadline)? {
                return Ok(Identification::from_json("firefly", json));
            }
        }

        // Phase 2: send `I` and read one line.
        buf.clear();
        {
            let mut guard = port.lock().map_err(|e| {
                IdentifyError::Io(format!("serial lock poisoned: {e}"))
            })?;
            use std::io::Write;
            guard
                .write_all(b"I\n")
                .map_err(|e| IdentifyError::Io(format!("write I: {e}")))?;
            guard
                .flush()
                .map_err(|e| IdentifyError::Io(format!("flush I: {e}")))?;
            let deadline = Instant::now() + HELLO_TIMEOUT;
            if let Some(json) = read_frame_until(&mut **guard, &mut buf, deadline)? {
                return Ok(Identification::from_json("firefly", json));
            }
        }

        Ok(None)
    }
}

/// Read bytes into `buf` until a parseable firefly identification
/// frame arrives, the deadline elapses, or the buffer exceeds
/// [`MAX_LINE_LEN`]. Returns `Some(json)` on a parseable line,
/// `None` on timeout, `Err` on a fatal read failure.
fn read_frame_until(
    port: &mut dyn serialport::SerialPort,
    buf: &mut Vec<u8>,
    deadline: Instant,
) -> Result<Option<serde_json::Value>, IdentifyError> {
    let mut scratch = [0u8; 128];
    while Instant::now() < deadline {
        match port.read(&mut scratch) {
            Ok(0) => {
                // No data yet; loop waits until either data arrives or
                // deadline elapses.
                continue;
            }
            Ok(n) => {
                if buf.len() + n > MAX_LINE_LEN {
                    return Err(IdentifyError::Malformed(
                        "identification frame exceeded maximum length".into(),
                    ));
                }
                buf.extend_from_slice(&scratch[..n]);
                // Look for a newline; parse each candidate line.
                while let Some(nl) = buf.iter().position(|&c| c == b'\n') {
                    let line: Vec<u8> = buf.drain(..=nl).collect();
                    let line = &line[..line.len().saturating_sub(1)];
                    match parse_identification_frame(line) {
                        Ok(json) => return Ok(Some(json)),
                        Err(_) => continue, // Not an identification line; keep reading.
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Inner timeout; loop until outer deadline.
                continue;
            }
            Err(e) => {
                return Err(IdentifyError::Io(format!("read: {e}")));
            }
        }
    }
    Ok(None)
}

/// Parse a single serial line as a firefly identification frame.
///
/// Accepts:
///   `* HELLO,<json>`  (unsolicited boot frame)
///   `OK,<json>`       (reply to the `I` command)
///
/// Returns the parsed JSON value on success; `Err(Malformed)` when
/// the prefix matched but the body didn't parse; structurally
/// `Err(…)` when the prefix doesn't match either flavour.
fn parse_identification_frame(line: &[u8]) -> Result<serde_json::Value, IdentifyError> {
    let line = std::str::from_utf8(line)
        .map_err(|e| IdentifyError::Malformed(format!("non-UTF8 line: {e}")))?
        .trim_end_matches('\r')
        .trim();

    let body = if let Some(rest) = line.strip_prefix("* HELLO,") {
        rest
    } else if let Some(rest) = line.strip_prefix("OK,") {
        // Only treat `OK,` payloads as identifications when they
        // begin with a JSON object — `OK,ready` and other plain OK
        // responses slip through as "not an identification."
        if rest.trim_start().starts_with('{') {
            rest
        } else {
            return Err(IdentifyError::Malformed(format!(
                "non-JSON OK line: {line}"
            )));
        }
    } else {
        return Err(IdentifyError::Malformed(format!(
            "line is neither HELLO nor OK: {line}"
        )));
    };

    serde_json::from_str(body)
        .map_err(|e| IdentifyError::Malformed(format!("body not JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_companion_sdk::bus::Device;
    use serde_json::json;

    #[test]
    fn parses_hello_frame() {
        let frame = b"* HELLO,{\"device_id\":\"01938abc-de01-7234-89ab-cdef01234567\",\"family\":\"firefly\",\"variant\":\"oled\"}";
        let value = parse_identification_frame(frame).unwrap();
        assert_eq!(value["family"], json!("firefly"));
        assert_eq!(value["variant"], json!("oled"));
    }

    #[test]
    fn parses_ok_reply() {
        let frame = b"OK,{\"device_id\":\"01938abc-de01-7234-89ab-cdef01234567\",\"family\":\"firefly\",\"variant\":\"tdisplay\"}";
        let value = parse_identification_frame(frame).unwrap();
        assert_eq!(value["variant"], json!("tdisplay"));
    }

    #[test]
    fn rejects_plain_ok_responses() {
        let frame = b"OK,ready";
        assert!(parse_identification_frame(frame).is_err());
    }

    #[test]
    fn rejects_unknown_prefix() {
        let frame = b"HELLO,{\"device_id\":\"x\"}";
        assert!(parse_identification_frame(frame).is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        let frame = b"* HELLO,{not json";
        assert!(parse_identification_frame(frame).is_err());
    }

    #[test]
    fn mock_device_path_parses_full_frame() {
        let proto = FireflyIdentityProtocol::new();
        let payload = format!(
            "* HELLO,{}\n",
            json!({
                "device_id": "01938abc-de01-7234-89ab-cdef01234567",
                "family": "firefly",
                "variant": "oled",
                "version": "0.2.0",
            })
        );
        let device = Device::usb_serial("test", 0x1a86, 0x7523, None, "/dev/ttyMOCK0");
        let mut opened = OpenedDevice::mock(device, payload.into_bytes());
        let id = proto.identify(&mut opened).unwrap().unwrap();
        assert_eq!(id.ecosystem, "firefly");
        assert_eq!(
            id.device_id,
            "01938abc-de01-7234-89ab-cdef01234567"
        );
        assert_eq!(id.string_field("variant"), Some("oled"));
    }

    #[test]
    fn mock_device_path_skips_when_no_identification() {
        let proto = FireflyIdentityProtocol::new();
        let device = Device::usb_serial("test", 0x1a86, 0x7523, None, "/dev/ttyMOCK0");
        let mut opened = OpenedDevice::mock(device, b"OK,ready\n".to_vec());
        // Mock path parses the whole buffer as a single frame; plain
        // `OK,ready` yields Err(Malformed).
        let err = proto.identify(&mut opened).unwrap_err();
        matches!(err, IdentifyError::Malformed(_));
    }
}
