//! Serial communication with Firefly RP2040-Matrix device
//!
//! Protocol: Text-based serial commands at 115200 baud
//! Commands are terminated with \n, responses are terminated with \r\n

use anyhow::{Context, Result};
use serialport::SerialPort;
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;
use std::time::Duration;

/// Firefly serial connection
pub struct FireflySerial {
    port: Mutex<Box<dyn SerialPort>>,
    port_name: String,
}

impl FireflySerial {
    /// Open serial connection to Firefly device
    pub fn new(port_name: &str) -> Result<Self> {
        let port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(1000))
            .open()
            .with_context(|| format!("Failed to open serial port {}", port_name))?;

        Ok(Self {
            port: Mutex::new(port),
            port_name: port_name.to_string(),
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

        // Read response (with timeout)
        let mut reader = BufReader::new(port.try_clone()?);
        let mut response = String::new();

        match reader.read_line(&mut response) {
            Ok(_) => {
                let response = response.trim().to_string();
                tracing::trace!(command = %command, response = %response, "Serial command");
                Ok(response)
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                tracing::warn!(command = %command, "Command timed out");
                Err(anyhow::anyhow!("Command timed out"))
            }
            Err(e) => Err(anyhow::anyhow!("Read error: {}", e)),
        }
    }

    /// Set single pixel
    pub fn pixel(&self, x: u8, y: u8, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_command(&format!("P,{},{},{},{},{}", x, y, r, g, b))
    }

    /// Fill all pixels with color
    pub fn fill(&self, r: u8, g: u8, b: u8) -> Result<String> {
        self.send_command(&format!("F,{},{},{}", r, g, b))
    }

    /// Clear display (all off)
    pub fn clear(&self) -> Result<String> {
        self.send_command("C")
    }

    /// Set brightness (0-100)
    pub fn brightness(&self, percent: u8) -> Result<String> {
        self.send_command(&format!("B,{}", percent.min(100)))
    }

    /// Start animation
    pub fn animate(&self, name: &str) -> Result<String> {
        self.send_command(&format!("A,{}", name))
    }

    /// Stop animation
    pub fn stop(&self) -> Result<String> {
        self.send_command("S")
    }

    /// Show status indicator
    pub fn status(&self, state: &str) -> Result<String> {
        self.send_command(&format!("T,{}", state))
    }

    /// Get device info
    pub fn info(&self) -> Result<String> {
        self.send_command("I")
    }

    /// Get port name
    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

/// Connection manager that handles device discovery and reconnection
pub struct FireflyConnection {
    serial: Mutex<Option<FireflySerial>>,
    preferred_port: Option<String>,
}

impl FireflyConnection {
    /// Create a new connection manager
    pub fn new(preferred_port: Option<String>) -> Self {
        Self {
            serial: Mutex::new(None),
            preferred_port,
        }
    }

    /// Try to connect to a device
    pub fn try_connect(&self) -> Result<()> {
        let port_name = match &self.preferred_port {
            Some(p) => p.clone(),
            None => find_firefly_port()?,
        };

        let serial = FireflySerial::new(&port_name)?;

        // Verify device responds
        let response = serial.send_command("I")?;
        if !response.starts_with("OK") {
            return Err(anyhow::anyhow!("Device did not respond correctly: {}", response));
        }

        tracing::info!(port = %port_name, response = %response, "Firefly device connected");

        let mut guard = self
            .serial
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        *guard = Some(serial);

        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.serial
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Get connection status info
    pub fn status_info(&self) -> String {
        match self.serial.lock() {
            Ok(guard) => match &*guard {
                Some(serial) => format!("Connected to {}", serial.port_name()),
                None => "Not connected".to_string(),
            },
            Err(_) => "Lock error".to_string(),
        }
    }

    /// Execute a command on the device if connected
    pub fn with_device<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&FireflySerial) -> Result<T>,
    {
        let guard = self
            .serial
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        match &*guard {
            Some(serial) => f(serial),
            None => Err(anyhow::anyhow!("No Firefly device connected")),
        }
    }

    /// Disconnect (mark as disconnected, e.g., after communication error)
    pub fn disconnect(&self) {
        if let Ok(mut guard) = self.serial.lock() {
            if guard.is_some() {
                tracing::warn!("Firefly device disconnected");
                *guard = None;
            }
        }
    }
}

/// Find RP2040-Matrix port automatically
pub fn find_firefly_port() -> Result<String> {
    let ports = serialport::available_ports()?;

    // Look for Raspberry Pi RP2040 (VID 0x2e8a)
    for port in &ports {
        if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
            if info.vid == 0x2e8a {
                tracing::debug!(
                    port = %port.port_name,
                    vid = format!("{:04x}", info.vid),
                    pid = format!("{:04x}", info.pid),
                    "Found RP2040 device"
                );
                return Ok(port.port_name.clone());
            }
        }
    }

    Err(anyhow::anyhow!("No RP2040-Matrix found"))
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
