//! [`Firefly`] — the firefly domain entity.
//!
//! Wraps an `Arc<UsbSerialDevice>` with the firefly protocol. Holds
//! parsed [`Identity`] from the `I` response. Exposes protocol
//! methods — `oled_health`, `matrix_fill`, `tdisplay_json_push`, etc.
//! — each of which builds a command string and routes it through
//! the device's `send()`, optionally awaiting a single response line
//! via `device.lines()`.
//!
//! Adapters hold `Arc<Firefly>` and speak firefly vocabulary; when
//! they want raw USB-level operations they reach through as
//! `firefly.device.send(...)` (legitimate per the Law of Instances:
//! `send` is USB-domain vocabulary being invoked on this firefly's
//! device).

use anyhow::{anyhow, Context, Result};
use garden_companion_usb::UsbSerialDevice;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Reply-await timeout for synchronous firefly commands.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);

// ---------------------------------------------------------------------------
// Firefly classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireflyKind {
    /// Waveshare RP2040 5×5 RGB matrix.
    Rp2040Matrix,
    /// ESP8266 128×64 SSD1306 OLED v1 (stone name / health / metrics).
    Esp8266Oled,
    /// ESP8266 128×64 SSD1306 OLED v2 (packed dashboard + icons).
    Esp8266OledV2,
    /// ESP32 135×240 ST7789 T-Display (full-color JSON push).
    Esp32TDisplay,
    /// Recognised firefly family but variant unknown.
    Unknown,
}

impl FireflyKind {
    fn classify(identity: &Identity) -> Self {
        match identity.variant.as_str() {
            "matrix" => Self::Rp2040Matrix,
            "tdisplay" => Self::Esp32TDisplay,
            "oled" => {
                if identity.has_capability("dashboard") {
                    Self::Esp8266OledV2
                } else {
                    Self::Esp8266Oled
                }
            }
            _ => Self::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Parsed firefly identification (from the `I` response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub family: String,
    pub variant: String,
    pub device_id: String,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    /// Full descriptor JSON for anything else an adapter wants to
    /// inspect (e.g. display dimensions).
    pub fields: Value,
}

impl Identity {
    pub fn parse(value: Value) -> Result<Self> {
        let family = value
            .get("family")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing family"))?
            .to_string();
        if family != "firefly" {
            return Err(anyhow!("not a firefly (family={family})"));
        }
        let variant = value
            .get("variant")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let device_id = value
            .get("device_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let version = value
            .get("version")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let capabilities = value
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Identity {
            family,
            variant,
            device_id,
            version,
            capabilities,
            fields: value,
        })
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c == name)
    }
}

// ---------------------------------------------------------------------------
// Firefly entity
// ---------------------------------------------------------------------------

pub struct Firefly {
    /// The underlying USB device. Public so adapters can invoke
    /// USB-domain vocabulary directly (e.g. `firefly.device.state_changes()`).
    pub device: Arc<UsbSerialDevice>,
    pub identity: Identity,
    pub kind: FireflyKind,
}

impl Firefly {
    pub fn new(device: Arc<UsbSerialDevice>, identity: Identity) -> Arc<Self> {
        let kind = FireflyKind::classify(&identity);
        Arc::new(Self {
            device,
            identity,
            kind,
        })
    }

    // ---- Low-level command helpers -------------------------------------

    /// Send a command without awaiting a response. Returns as soon
    /// as the driver has written + flushed the bytes.
    pub async fn send_no_wait(&self, command: &str) -> Result<()> {
        let bytes = format!("{}\n", command);
        self.device.send(bytes.as_bytes()).await
    }

    /// Send a command and await the next line as response. Subscribes
    /// to the device's broadcast *before* sending to avoid losing the
    /// reply to a TOCTOU race.
    pub async fn send_await(&self, command: &str) -> Result<String> {
        let mut rx = self.device.lines();
        self.send_no_wait(command).await?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(anyhow!("command '{command}' timed out"));
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(line)) => return Ok(line),
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    return Err(anyhow!("line stream closed"));
                }
                Err(_) => return Err(anyhow!("command '{command}' timed out")),
            }
        }
    }

    // ---- Common commands -----------------------------------------------

    pub async fn clear(&self) -> Result<String> {
        self.send_await("C").await
    }

    pub async fn brightness(&self, percent: u8) -> Result<String> {
        self.send_await(&format!("B,{}", percent.min(100))).await
    }

    pub async fn info(&self) -> Result<String> {
        self.send_await("I").await
    }

    // ---- RP2040 matrix -------------------------------------------------

    pub async fn pixel(&self, x: u8, y: u8, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_await(&format!("P,{},{},{},{},{}", x, y, r, g, b))
            .await
    }

    pub async fn fill(&self, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_await(&format!("F,{},{},{}", r, g, b)).await
    }

    pub async fn animate(&self, name: &str) -> Result<String> {
        self.send_await(&format!("A,{}", name)).await
    }

    pub async fn stop(&self) -> Result<String> {
        self.send_await("S").await
    }

    pub async fn status(&self, state: &str) -> Result<String> {
        self.send_await(&format!("T,{}", state)).await
    }

    // ---- OLED v1 -------------------------------------------------------

    pub async fn oled_stone_name(&self, name: &str) -> Result<String> {
        self.send_await(&format!("S,{}", name)).await
    }

    pub async fn oled_health(&self, state: &str) -> Result<String> {
        self.send_await(&format!("H,{}", state)).await
    }

    pub async fn oled_metrics(&self, cpu: u8, mem: u8, uptime: &str) -> Result<String> {
        self.send_await(&format!("M,{},{},{}", cpu.min(100), mem.min(100), uptime))
            .await
    }

    pub async fn oled_wipe_in(&self, line1: &str, line2: &str) -> Result<()> {
        self.send_no_wait(&format!("WIPE-IN,{},{}", line1, line2)).await
    }

    pub async fn oled_wipe_out(&self, line1: &str, line2: &str) -> Result<()> {
        self.send_no_wait(&format!("WIPE-OUT,{},{}", line1, line2)).await
    }

    // ---- OLED v2 dashboard --------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn oled_v2_dashboard(
        &self,
        cpu: u8,
        mem: u8,
        disk: u8,
        uptime: &str,
        offerings: usize,
        stones: usize,
        net_bps: u64,
        seed_bank: bool,
    ) -> Result<String> {
        self.send_await(&format!(
            "D,{},{},{},{},{},{},{},{}",
            cpu.min(100),
            mem.min(100),
            disk.min(100),
            uptime,
            offerings,
            stones,
            net_bps,
            if seed_bank { 1 } else { 0 },
        ))
        .await
    }

    // ---- T-Display -----------------------------------------------------

    pub async fn tdisplay_json_push(&self, json: &str) -> Result<String> {
        self.send_await(&format!("J,{}", json)).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tdisplay_load(
        &self,
        cpu: u8,
        mem: u8,
        disk: u8,
        io: u8,
        gpu: u8,
        gpu_active: bool,
    ) -> Result<String> {
        self.send_await(&format!(
            "L,{},{},{},{},{},{}",
            cpu,
            mem,
            disk,
            io,
            gpu,
            if gpu_active { 1 } else { 0 }
        ))
        .await
    }

    pub async fn tdisplay_health(&self, health: &str) -> Result<String> {
        self.send_await(&format!("H,{}", health)).await
    }

    pub async fn tdisplay_service_started(&self, name: &str, health: &str) -> Result<()> {
        let health_char = match health {
            "healthy" => "h",
            "unhealthy" | "withering" => "w",
            _ => "h",
        };
        self.send_no_wait(&format!("+,{},{}", name, health_char)).await
    }

    pub async fn tdisplay_service_stopped(&self, name: &str) -> Result<()> {
        self.send_no_wait(&format!("-,{}", name)).await
    }

    pub async fn tdisplay_tended(&self, client: &str, host: &str) -> Result<()> {
        self.send_no_wait(&format!("T,{},{}", client, host)).await
    }

    pub async fn tdisplay_seed_bank_detected(&self, name: &str, used: u64, total: u64) -> Result<()> {
        self.send_no_wait(&format!("SD,{},{},{}", name, used, total)).await
    }

    pub async fn tdisplay_seed_bank_removed(&self) -> Result<()> {
        self.send_no_wait("SR").await
    }
}

// ---------------------------------------------------------------------------
// Color parsing helper
// ---------------------------------------------------------------------------

pub fn parse_color(color: &str) -> Result<(u8, u8, u8)> {
    let color = color.trim().to_lowercase();
    let hex = color.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;
        return Ok((r, g, b));
    }
    let parts: Vec<&str> = color.split(',').collect();
    if parts.len() == 3 {
        let r: u8 = parts[0].trim().parse().context("red")?;
        let g: u8 = parts[1].trim().parse().context("green")?;
        let b: u8 = parts[2].trim().parse().context("blue")?;
        return Ok((r, g, b));
    }
    match color.as_str() {
        "red" => Ok((255, 0, 0)),
        "green" => Ok((0, 255, 0)),
        "blue" => Ok((0, 0, 255)),
        "white" => Ok((255, 255, 255)),
        "yellow" => Ok((255, 255, 0)),
        "cyan" => Ok((0, 255, 255)),
        "magenta" => Ok((255, 0, 255)),
        "orange" => Ok((255, 165, 0)),
        "off" | "black" => Ok((0, 0, 0)),
        _ => Err(anyhow!("invalid color (use hex, r,g,b, or named)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_parse_accepts_full_frame() {
        let v = json!({
            "family": "firefly",
            "variant": "oled",
            "device_id": "abc",
            "version": "2.0.0",
            "capabilities": ["dashboard", "brightness"]
        });
        let id = Identity::parse(v).unwrap();
        assert_eq!(id.variant, "oled");
        assert!(id.has_capability("dashboard"));
        assert_eq!(FireflyKind::classify(&id), FireflyKind::Esp8266OledV2);
    }

    #[test]
    fn identity_parse_rejects_non_firefly() {
        let v = json!({ "family": "cricket", "variant": "x" });
        assert!(Identity::parse(v).is_err());
    }

    #[test]
    fn firefly_kind_classifies_oled_v1_without_dashboard() {
        let id = Identity::parse(json!({
            "family": "firefly",
            "variant": "oled",
            "device_id": "d",
            "capabilities": []
        }))
        .unwrap();
        assert_eq!(FireflyKind::classify(&id), FireflyKind::Esp8266Oled);
    }

    #[test]
    fn parse_color_hex_rgb_and_names() {
        assert_eq!(parse_color("ff0000").unwrap(), (255, 0, 0));
        assert_eq!(parse_color("#00ff00").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("0, 0, 255").unwrap(), (0, 0, 255));
        assert_eq!(parse_color("magenta").unwrap(), (255, 0, 255));
        assert!(parse_color("nope").is_err());
    }
}
