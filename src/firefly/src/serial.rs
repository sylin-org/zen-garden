//! Serial communication with Firefly devices
//!
//! Supports multiple device types:
//! - RP2040-Matrix: 5×5 RGB LED matrix (VID: 0x2e8a Raspberry Pi, 0x239a Adafruit)
//! - ESP8266-OLED: 128×64 SSD1306 OLED display (VID: 0x1a86 CH340)
//!
//! Protocol: Text-based serial commands at 115200 baud
//! Commands are terminated with \n, responses are terminated with \r\n

use anyhow::{Context, Result};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::Duration;

/// Known USB Vendor IDs
const VID_RASPBERRY_PI: u16 = 0x2e8a; // Native RP2040 / Pico SDK
const VID_ADAFRUIT: u16 = 0x239a; // CircuitPython firmware
const VID_CH340: u16 = 0x1a86; // CH340 (ESP8266 NodeMCU)

/// Detected Firefly device type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FireflyDeviceType {
    /// Waveshare RP2040-Matrix with 5×5 RGB LED matrix
    Rp2040Matrix,
    /// ESP8266 NodeMCU with 128×64 SSD1306 OLED display
    Esp8266Oled,
    /// Unknown device type (responds to protocol but unrecognized VID)
    #[default]
    Unknown,
}

impl std::fmt::Display for FireflyDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FireflyDeviceType::Rp2040Matrix => write!(f, "RP2040-Matrix"),
            FireflyDeviceType::Esp8266Oled => write!(f, "ESP8266-OLED"),
            FireflyDeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

impl FireflyDeviceType {
    /// Classify device type from USB VID
    pub fn from_vid(vid: u16) -> Self {
        match vid {
            VID_RASPBERRY_PI | VID_ADAFRUIT => FireflyDeviceType::Rp2040Matrix,
            VID_CH340 => FireflyDeviceType::Esp8266Oled,
            _ => FireflyDeviceType::Unknown,
        }
    }
}

/// Information about a detected Firefly device
#[derive(Debug, Clone)]
pub struct DetectedDevice {
    pub port_name: String,
    pub device_type: FireflyDeviceType,
    pub vid: u16,
    pub pid: u16,
}

/// Firefly serial connection
pub struct FireflySerial {
    port: Mutex<Box<dyn SerialPort>>,
    port_name: String,
    device_type: FireflyDeviceType,
}

impl FireflySerial {
    /// Open serial connection to Firefly device
    pub fn new(port_name: &str, device_type: FireflyDeviceType) -> Result<Self> {
        let port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(2000)) // Longer timeout for ESP8266 boot
            .open()
            .with_context(|| format!("Failed to open serial port {}", port_name))?;

        // For ESP8266: Opening the port toggles DTR which resets the device.
        // We need to wait for it to boot and print "OK,ready" before sending commands.
        if device_type == FireflyDeviceType::Esp8266Oled {
            tracing::debug!("Waiting for ESP8266 to boot...");

            // Wait longer for ESP8266 to boot (MicroPython takes ~1-2s)
            std::thread::sleep(Duration::from_millis(2000));

            // Clear any boot garbage from input buffer
            let _ = port.clear(serialport::ClearBuffer::Input);

            tracing::debug!("ESP8266 boot wait complete, buffer cleared");
        }

        Ok(Self {
            port: Mutex::new(port),
            port_name: port_name.to_string(),
            device_type,
        })
    }

    /// Send command and wait for response
    pub fn send_command(&self, command: &str) -> Result<String> {
        let mut port = self
            .port
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Clear any pending input
        let _ = port.clear(serialport::ClearBuffer::Input);

        // Send command with newline
        let cmd_bytes = format!("{}\n", command);
        port.write_all(cmd_bytes.as_bytes())
            .context("Failed to write command")?;
        port.flush().context("Failed to flush")?;

        // Read response byte-by-byte until newline (avoids BufReader buffering issues)
        let mut response = Vec::with_capacity(256);
        let mut buf = [0u8; 1];

        loop {
            match port.read(&mut buf) {
                Ok(1) => {
                    if buf[0] == b'\n' {
                        break;
                    }
                    if buf[0] != b'\r' {
                        response.push(buf[0]);
                    }
                }
                Ok(_) => continue, // 0 bytes read, retry
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    tracing::warn!(command = %command, "Command timed out");
                    return Err(anyhow::anyhow!("Command timed out"));
                }
                Err(e) => return Err(anyhow::anyhow!("Read error: {}", e)),
            }
        }

        let response = String::from_utf8_lossy(&response).trim().to_string();
        tracing::trace!(command = %command, response = %response, "Serial command");
        Ok(response)
    }

    /// Get device type
    pub fn device_type(&self) -> FireflyDeviceType {
        self.device_type
    }

    // ==================== Common Commands ====================

    /// Clear display (all off)
    pub fn clear(&self) -> Result<String> {
        self.send_command("C")
    }

    /// Set brightness (0-100)
    pub fn brightness(&self, percent: u8) -> Result<String> {
        self.send_command(&format!("B,{}", percent.min(100)))
    }

    /// Get device info
    pub fn info(&self) -> Result<String> {
        self.send_command("I")
    }

    /// Get port name
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    // ==================== RP2040 Matrix Commands ====================

    /// Set single pixel (RP2040 only)
    pub fn pixel(&self, x: u8, y: u8, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_command(&format!("P,{},{},{},{},{}", x, y, r, g, b))
    }

    /// Fill all pixels with color (RP2040 only)
    pub fn fill(&self, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_command(&format!("F,{},{},{}", r, g, b))
    }

    /// Start animation (RP2040 only)
    pub fn animate(&self, name: &str) -> Result<String> {
        self.send_command(&format!("A,{}", name))
    }

    /// Stop animation (RP2040 only, 'S' is used for stone name on OLED)
    pub fn stop(&self) -> Result<String> {
        self.send_command("S")
    }

    /// Show status indicator (RP2040 only)
    pub fn status(&self, state: &str) -> Result<String> {
        self.send_command(&format!("T,{}", state))
    }

    // ==================== ESP8266 OLED Commands ====================

    /// Set stone name (OLED only)
    pub fn oled_stone_name(&self, name: &str) -> Result<String> {
        self.send_command(&format!("S,{}", name))
    }

    /// Set health state (OLED only): thriving, withering, wilting, resting
    pub fn oled_health(&self, state: &str) -> Result<String> {
        self.send_command(&format!("H,{}", state))
    }

    /// Update metrics (OLED only): cpu, memory (0-100), uptime string
    pub fn oled_metrics(&self, cpu: u8, mem: u8, uptime: &str) -> Result<String> {
        self.send_command(&format!("M,{},{},{}", cpu.min(100), mem.min(100), uptime))
    }

    /// Refresh OLED display
    pub fn oled_refresh(&self) -> Result<String> {
        self.send_command("R")
    }

    /// Wipe-in animation (OLED only) - fire-and-forget since animation takes time
    pub fn oled_wipe_in(&self, line1: &str, line2: &str) -> Result<String> {
        // Wipe animations take ~400ms, so don't wait for response
        self.send_command_no_wait(&format!("WIPE-IN,{},{}", line1, line2))
    }

    /// Wipe-out animation (OLED only) - fire-and-forget since animation takes time
    pub fn oled_wipe_out(&self, line1: &str, line2: &str) -> Result<String> {
        // Wipe animations take ~400ms, so don't wait for response
        self.send_command_no_wait(&format!("WIPE-OUT,{},{}", line1, line2))
    }

    /// Blink animation (OLED only) - fire-and-forget since animation takes time
    pub fn oled_blink(&self, count: u8) -> Result<String> {
        // Blink animations take ~300ms per blink, so don't wait for response
        self.send_command_no_wait(&format!("BLINK,{}", count))
    }

    /// Pulse animation (OLED only) - fire-and-forget since animation takes time
    pub fn oled_pulse(&self, count: u8) -> Result<String> {
        // Pulse animations take ~500ms per pulse, so don't wait for response
        self.send_command_no_wait(&format!("PULSE,{}", count))
    }

    /// Send command without waiting for response (for long-running animations)
    fn send_command_no_wait(&self, command: &str) -> Result<String> {
        let mut port = self
            .port
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Clear any pending input
        let _ = port.clear(serialport::ClearBuffer::Input);

        // Send command with newline
        let cmd_bytes = format!("{}\n", command);
        port.write_all(cmd_bytes.as_bytes())
            .context("Failed to write command")?;
        port.flush().context("Failed to flush")?;

        tracing::trace!(command = %command, "Serial command (no wait)");
        Ok("OK".to_string())
    }
}

/// Connection manager that handles device discovery and reconnection
pub struct FireflyConnection {
    serial: Mutex<Option<FireflySerial>>,
    device_type: Mutex<FireflyDeviceType>,
    preferred_port: Option<String>,
}

impl FireflyConnection {
    /// Create a new connection manager
    pub fn new(preferred_port: Option<String>) -> Self {
        Self {
            serial: Mutex::new(None),
            device_type: Mutex::new(FireflyDeviceType::Unknown),
            preferred_port,
        }
    }

    /// Try to connect to a device
    pub fn try_connect(&self) -> Result<()> {
        let detected = match &self.preferred_port {
            Some(p) => {
                // If port is specified, detect its type
                let device_type = detect_device_type(p).unwrap_or(FireflyDeviceType::Unknown);
                DetectedDevice {
                    port_name: p.clone(),
                    device_type,
                    vid: 0,
                    pid: 0,
                }
            }
            None => find_firefly_device()?,
        };

        let serial = FireflySerial::new(&detected.port_name, detected.device_type)?;

        // Verify device responds - retry once for ESP8266 which may need more boot time
        let response = match serial.send_command("I") {
            Ok(resp) => resp,
            Err(e) if detected.device_type == FireflyDeviceType::Esp8266Oled => {
                tracing::debug!(error = %e, "First info command failed, retrying after delay");
                std::thread::sleep(Duration::from_millis(500));
                serial.send_command("I")?
            }
            Err(e) => return Err(e),
        };

        if !response.starts_with("OK") {
            return Err(anyhow::anyhow!(
                "Device did not respond correctly: {}",
                response
            ));
        }

        tracing::info!(
            port = %detected.port_name,
            device_type = %detected.device_type,
            response = %response,
            "Firefly device connected"
        );

        // Update device type
        {
            let mut dt = self
                .device_type
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            *dt = detected.device_type;
        }

        let mut guard = self
            .serial
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        *guard = Some(serial);

        Ok(())
    }

    /// Disconnect from device (clears connection state)
    pub fn disconnect(&self) {
        if let Ok(mut guard) = self.serial.lock() {
            if guard.is_some() {
                tracing::info!("Firefly device disconnected");
            }
            *guard = None;
        }
        if let Ok(mut dt) = self.device_type.lock() {
            *dt = FireflyDeviceType::Unknown;
        }
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.serial
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Get the connected device type
    pub fn device_type(&self) -> FireflyDeviceType {
        self.device_type
            .lock()
            .map(|guard| *guard)
            .unwrap_or(FireflyDeviceType::Unknown)
    }

    /// Get connection status info
    pub fn status_info(&self) -> String {
        match self.serial.lock() {
            Ok(guard) => match &*guard {
                Some(serial) => format!(
                    "Connected to {} ({})",
                    serial.port_name(),
                    serial.device_type()
                ),
                None => "Not connected".to_string(),
            },
            Err(_) => "Lock error".to_string(),
        }
    }

    /// Execute a command on the device if connected.
    /// If the command fails with an I/O error (device unplugged), disconnects automatically.
    pub fn with_device<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&FireflySerial) -> Result<T>,
    {
        let result = {
            let guard = self
                .serial
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

            match &*guard {
                Some(serial) => f(serial),
                None => return Err(anyhow::anyhow!("No Firefly device connected")),
            }
        };

        // Check if the error indicates disconnection
        if let Err(ref e) = result {
            let error_str = e.to_string().to_lowercase();
            // Detect disconnection errors: timeout, I/O errors, device not found
            if error_str.contains("timed out")
                || error_str.contains("i/o error")
                || error_str.contains("access is denied")
                || error_str.contains("device not found")
                || error_str.contains("no such file")
                || error_str.contains("broken pipe")
                || error_str.contains("connection reset")
            {
                tracing::warn!(error = %e, "Device communication failed, marking as disconnected");
                self.disconnect();
            }
        }

        result
    }
}

/// Detect device type from a specific port
pub fn detect_device_type(port_name: &str) -> Result<FireflyDeviceType> {
    let ports = serialport::available_ports()?;

    for port in &ports {
        // Case-insensitive comparison for Windows (COM3 vs com3)
        if port.port_name.eq_ignore_ascii_case(port_name) {
            if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
                return Ok(FireflyDeviceType::from_vid(info.vid));
            }
        }
    }

    Ok(FireflyDeviceType::Unknown)
}

/// Find any supported Firefly device (RP2040 or ESP8266)
pub fn find_firefly_device() -> Result<DetectedDevice> {
    let ports = serialport::available_ports()?;

    tracing::debug!(port_count = ports.len(), "Scanning for Firefly devices");

    // Priority: RP2040 first, then ESP8266
    let mut candidates: Vec<DetectedDevice> = Vec::new();

    for port in &ports {
        match &port.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                let device_type = FireflyDeviceType::from_vid(info.vid);
                tracing::debug!(
                    port = %port.port_name,
                    vid = format!("{:04x}", info.vid),
                    pid = format!("{:04x}", info.pid),
                    device_type = %device_type,
                    product = info.product.as_deref().unwrap_or("unknown"),
                    "Found USB serial port"
                );
                if device_type != FireflyDeviceType::Unknown {
                    candidates.push(DetectedDevice {
                        port_name: port.port_name.clone(),
                        device_type,
                        vid: info.vid,
                        pid: info.pid,
                    });
                }
            }
            other => {
                tracing::trace!(
                    port = %port.port_name,
                    port_type = ?other,
                    "Skipping non-USB port"
                );
            }
        }
    }

    // Prefer RP2040 over ESP8266 (original device type)
    candidates.sort_by_key(|d| match d.device_type {
        FireflyDeviceType::Rp2040Matrix => 0,
        FireflyDeviceType::Esp8266Oled => 1,
        FireflyDeviceType::Unknown => 2,
    });

    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Firefly device found"))
}

/// Find RP2040-Matrix port automatically (legacy, prefer find_firefly_device)
pub fn find_firefly_port() -> Result<String> {
    find_firefly_device().map(|d| d.port_name)
}

/// Parse color string (hex or r,g,b format)
pub fn parse_color(color: &str) -> Result<(u8, u8, u8)> {
    let color = color.trim().to_lowercase();

    // Try hex format (ff0000 or #ff0000)
    let hex = color.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;
        return Ok((r, g, b));
    }

    // Try r,g,b format
    let parts: Vec<&str> = color.split(',').collect();
    if parts.len() == 3 {
        let r: u8 = parts[0].trim().parse().context("Invalid red value")?;
        let g: u8 = parts[1].trim().parse().context("Invalid green value")?;
        let b: u8 = parts[2].trim().parse().context("Invalid blue value")?;
        return Ok((r, g, b));
    }

    // Named colors
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
        _ => Err(anyhow::anyhow!(
            "Invalid color format. Use hex (ff0000), r,g,b (255,0,0), or name (red)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_from_vid() {
        assert_eq!(
            FireflyDeviceType::from_vid(0x2e8a),
            FireflyDeviceType::Rp2040Matrix
        );
        assert_eq!(
            FireflyDeviceType::from_vid(0x239a),
            FireflyDeviceType::Rp2040Matrix
        );
        assert_eq!(
            FireflyDeviceType::from_vid(0x1a86),
            FireflyDeviceType::Esp8266Oled
        );
        assert_eq!(
            FireflyDeviceType::from_vid(0x0000),
            FireflyDeviceType::Unknown
        );
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("ff0000").unwrap(), (255, 0, 0));
        assert_eq!(parse_color("00ff00").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("0000ff").unwrap(), (0, 0, 255));
        assert_eq!(parse_color("#ffffff").unwrap(), (255, 255, 255));
        assert_eq!(parse_color("FF00FF").unwrap(), (255, 0, 255));
    }

    #[test]
    fn test_parse_color_rgb() {
        assert_eq!(parse_color("255,0,0").unwrap(), (255, 0, 0));
        assert_eq!(parse_color("0, 255, 0").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("128,128,128").unwrap(), (128, 128, 128));
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("red").unwrap(), (255, 0, 0));
        assert_eq!(parse_color("GREEN").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("Blue").unwrap(), (0, 0, 255));
        assert_eq!(parse_color("off").unwrap(), (0, 0, 0));
    }

    #[test]
    fn test_parse_color_invalid() {
        assert!(parse_color("invalid").is_err());
        assert!(parse_color("gggggg").is_err());
        assert!(parse_color("1,2").is_err());
    }
}
