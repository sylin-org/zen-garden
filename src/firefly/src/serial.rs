//! Serial communication with Firefly devices
//!
//! Supports multiple device types:
//! - RP2040-Matrix: 5×5 RGB LED matrix (VID: 0x2e8a Raspberry Pi, 0x239a Adafruit)
//! - ESP8266-OLED: 128×64 SSD1306 OLED display (VID: 0x1a86 CH340)
//! - ESP32-TDisplay: 135×240 ST7789 color TFT (VID: 0x1a86 CH9102) (FIREFLY-0003)
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
    /// ESP32 T-Display with 135×240 ST7789 color TFT (FIREFLY-0003)
    Esp32TDisplay,
    /// Unknown device type (responds to protocol but unrecognized VID)
    #[default]
    Unknown,
}

impl std::fmt::Display for FireflyDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FireflyDeviceType::Rp2040Matrix => write!(f, "RP2040-Matrix"),
            FireflyDeviceType::Esp8266Oled => write!(f, "ESP8266-OLED"),
            FireflyDeviceType::Esp32TDisplay => write!(f, "ESP32-TDisplay"),
            FireflyDeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

impl FireflyDeviceType {
    /// Classify device type from USB VID.
    ///
    /// Note: CH340 (ESP8266-OLED) and CH9102 (ESP32-TDisplay) share VID 0x1a86.
    /// Initial classification defaults to ESP8266; `refine_from_info()` upgrades
    /// to TDisplay after the `I` command returns `firefly-tdisplay`.
    pub fn from_vid(vid: u16) -> Self {
        match vid {
            VID_RASPBERRY_PI | VID_ADAFRUIT => FireflyDeviceType::Rp2040Matrix,
            VID_CH340 => FireflyDeviceType::Esp8266Oled, // Refined later for TDisplay
            _ => FireflyDeviceType::Unknown,
        }
    }

    /// Refine device type from info response (FIREFLY-0003).
    /// CH340 and CH9102 share VID; the `I` response distinguishes them.
    pub fn refine_from_info(current: Self, info_response: &str) -> Self {
        if info_response.contains("firefly-tdisplay") {
            FireflyDeviceType::Esp32TDisplay
        } else {
            current
        }
    }
}

/// Information about a detected Firefly device
#[derive(Debug, Clone)]
pub struct DetectedDevice {
    pub port_name: String,
    pub device_type: FireflyDeviceType,
    #[allow(dead_code)] // Stored for diagnostics/logging
    pub vid: u16,
    #[allow(dead_code)] // Stored for diagnostics/logging
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
        // Standard serial settings: 115200 8N1, no flow control
        let mut port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(2000))
            .data_bits(serialport::DataBits::Eight)
            .stop_bits(serialport::StopBits::One)
            .parity(serialport::Parity::None)
            .flow_control(serialport::FlowControl::None)
            .open()
            .with_context(|| format!("Failed to open serial port {}", port_name))?;

        // CircuitPython (RP2040) requires DTR/RTS asserted before it will transmit data.
        if device_type == FireflyDeviceType::Rp2040Matrix {
            if let Err(e) = port.write_data_terminal_ready(true) {
                tracing::debug!(error = %e, "Failed to set DTR true on RP2040");
            }
            if let Err(e) = port.write_request_to_send(true) {
                tracing::debug!(error = %e, "Failed to set RTS true on RP2040");
            }
        }

        // Log port settings for diagnostics
        tracing::debug!(
            port = %port_name,
            baud = 115200,
            settings = "8N1",
            flow_control = "none",
            "Serial port opened"
        );

        // Stabilization: all devices need a moment after port open
        // Opening a serial port can toggle DTR/RTS which may reset the device
        let stabilize_ms = match device_type {
            FireflyDeviceType::Esp8266Oled => 2000, // MicroPython boot takes longer
            FireflyDeviceType::Esp32TDisplay => 2000, // ESP32 MicroPython boot
            FireflyDeviceType::Rp2040Matrix => 2000, // CircuitPython boot + animation
            FireflyDeviceType::Unknown => 500,      // Conservative default
        };

        tracing::debug!(
            device_type = %device_type,
            stabilize_ms = stabilize_ms,
            "Waiting for device stabilization"
        );
        std::thread::sleep(Duration::from_millis(stabilize_ms));

        // Clear buffers to start fresh
        let _ = port.clear(serialport::ClearBuffer::All);

        // Wake-up sequence: send newline to clear any partial command state
        // Some devices may be waiting for input or have garbage in their buffer
        let _ = port.write_all(b"\n");
        let _ = port.flush();
        std::thread::sleep(Duration::from_millis(50));
        let _ = port.clear(serialport::ClearBuffer::Input); // Discard any response to wake-up

        tracing::debug!("Serial port ready, buffers cleared");

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
                    // Debug level - callers decide severity based on context
                    // (timeouts are expected during device detection/reconnection)
                    tracing::debug!(command = %command, "Command timed out");
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

    // ==================== ESP32 T-Display Commands (FIREFLY-0003) ====================

    /// Send full state JSON push (T-Display only)
    pub fn tdisplay_json_push(&self, json: &str) -> Result<String> {
        self.send_command(&format!("J,{}", json))
    }

    /// Send incremental load update (T-Display only)
    pub fn tdisplay_load(
        &self,
        cpu: u8,
        mem: u8,
        disk: u8,
        io: u8,
        gpu: u8,
        gpu_active: bool,
    ) -> Result<String> {
        self.send_command(&format!(
            "L,{},{},{},{},{},{}",
            cpu,
            mem,
            disk,
            io,
            gpu,
            if gpu_active { 1 } else { 0 }
        ))
    }

    /// Send health change (T-Display only)
    pub fn tdisplay_health(&self, health: &str) -> Result<String> {
        self.send_command(&format!("H,{}", health))
    }

    /// Send service started (T-Display only)
    pub fn tdisplay_service_started(&self, name: &str, health: &str) -> Result<String> {
        let health_char = match health {
            "healthy" => "h",
            "unhealthy" | "withering" => "w",
            _ => "h",
        };
        self.send_command_no_wait(&format!("+,{},{}", name, health_char))
    }

    /// Send service stopped (T-Display only)
    pub fn tdisplay_service_stopped(&self, name: &str) -> Result<String> {
        self.send_command_no_wait(&format!("-,{}", name))
    }

    /// Send stone tended (T-Display only)
    pub fn tdisplay_tended(&self, client: &str, host: &str) -> Result<String> {
        self.send_command_no_wait(&format!("T,{},{}", client, host))
    }

    /// Send seed bank detected (T-Display only)
    pub fn tdisplay_seed_bank_detected(
        &self,
        name: &str,
        used_gb: u64,
        total_gb: u64,
    ) -> Result<String> {
        self.send_command_no_wait(&format!("SD,{},{},{}", name, used_gb, total_gb))
    }

    /// Send seed bank removed (T-Display only)
    pub fn tdisplay_seed_bank_removed(&self) -> Result<String> {
        self.send_command_no_wait("SR")
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
    #[allow(dead_code)] // Protocol command for API completeness
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
    #[allow(dead_code)] // Protocol command for API completeness
    pub fn oled_blink(&self, count: u8) -> Result<String> {
        // Blink animations take ~300ms per blink, so don't wait for response
        self.send_command_no_wait(&format!("BLINK,{}", count))
    }

    /// Pulse animation (OLED only) - fire-and-forget since animation takes time
    #[allow(dead_code)] // Protocol command for API completeness
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
        let candidates = match &self.preferred_port {
            Some(p) => {
                let device_type = detect_device_type(p).unwrap_or(FireflyDeviceType::Unknown);
                tracing::info!(port = %p, device_type = %device_type, "Trying specified port");
                vec![DetectedDevice {
                    port_name: p.clone(),
                    device_type,
                    vid: 0,
                    pid: 0,
                }]
            }
            None => find_firefly_devices()?,
        };

        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No Firefly device found"));
        }

        let mut last_error = None;

        for detected in candidates {
            tracing::info!(
                port = %detected.port_name,
                device_type = %detected.device_type,
                vid = format!("{:04x}", detected.vid),
                pid = format!("{:04x}", detected.pid),
                "Found candidate device, verifying protocol"
            );

            let serial = match FireflySerial::new(&detected.port_name, detected.device_type) {
                Ok(s) => s,
                Err(e) => {
                    tracing::info!(
                        port = %detected.port_name,
                        error = %e,
                        "Failed to open device, trying next candidate"
                    );
                    last_error = Some(e);
                    continue;
                }
            };

            match verify_protocol(&serial, &detected) {
                Ok(response) => {
                    // FIREFLY-0003: Refine device type from info response
                    // CH340 (ESP8266-OLED) and CH9102 (ESP32-TDisplay) share VID 0x1a86.
                    let refined_type =
                        FireflyDeviceType::refine_from_info(detected.device_type, &response);

                    tracing::info!(
                        port = %detected.port_name,
                        device_type = %refined_type,
                        response = %response,
                        "Firefly device connected"
                    );

                    {
                        let mut dt = self
                            .device_type
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
                        *dt = refined_type;
                    }

                    let mut guard = self
                        .serial
                        .lock()
                        .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
                    *guard = Some(serial);

                    return Ok(());
                }
                Err(e) => {
                    tracing::info!(
                        port = %detected.port_name,
                        device_type = %detected.device_type,
                        error = %e,
                        "Device does not respond to Firefly protocol, trying next candidate"
                    );
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No Firefly device found")))
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
        if port.port_name.eq_ignore_ascii_case(port_name)
            && let serialport::SerialPortType::UsbPort(info) = &port.port_type {
                return Ok(FireflyDeviceType::from_vid(info.vid));
            }
    }

    Ok(FireflyDeviceType::Unknown)
}

/// Find any supported Firefly device (RP2040 or ESP8266)
pub fn find_firefly_devices() -> Result<Vec<DetectedDevice>> {
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

    // Prefer RP2040 over ESP devices (original device type)
    candidates.sort_by_key(|d| match d.device_type {
        FireflyDeviceType::Rp2040Matrix => 0,
        FireflyDeviceType::Esp32TDisplay => 1,
        FireflyDeviceType::Esp8266Oled => 2,
        FireflyDeviceType::Unknown => 3,
    });

    Ok(candidates)
}

pub fn find_firefly_device() -> Result<DetectedDevice> {
    find_firefly_devices()?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Firefly device found"))
}

/// Find RP2040-Matrix port automatically (legacy, prefer find_firefly_device)
#[allow(dead_code)] // Legacy API, kept for backwards compatibility
pub fn find_firefly_port() -> Result<String> {
    find_firefly_device().map(|d| d.port_name)
}

fn verify_protocol(serial: &FireflySerial, detected: &DetectedDevice) -> Result<String> {
    // Verify device responds to Firefly protocol with retry logic
    // Retry delays increase: 100ms, 300ms, 500ms
    const MAX_RETRIES: usize = 3;
    let retry_delays = [100, 300, 500];

    let mut last_error = None;
    let mut response = None;

    for attempt in 0..=MAX_RETRIES {
        match serial.send_command("I") {
            Ok(resp) => {
                response = Some(resp);
                break;
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES {
                    let delay = retry_delays.get(attempt).copied().unwrap_or(500);
                    tracing::debug!(
                        attempt = attempt + 1,
                        max = MAX_RETRIES + 1,
                        delay_ms = delay,
                        "Info command failed, retrying"
                    );
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }

    let response = match response {
        Some(r) => r,
        None => {
            let error = last_error.unwrap_or_else(|| anyhow::anyhow!("Unknown error"));
            tracing::info!(
                port = %detected.port_name,
                device_type = %detected.device_type,
                vid = format!("{:04x}", detected.vid),
                error = %error,
                "Device does not respond to Firefly protocol (may have incompatible firmware)"
            );
            return Err(anyhow::anyhow!(
                "Device on {} does not respond to Firefly protocol after {} attempts: {}",
                detected.port_name,
                MAX_RETRIES + 1,
                error
            ));
        }
    };

    if !response.starts_with("OK") {
        tracing::info!(
            port = %detected.port_name,
            response = %response,
            "Device responded but not with Firefly protocol (incompatible firmware)"
        );
        return Err(anyhow::anyhow!(
            "Device did not respond with Firefly protocol. Got: {}",
            response
        ));
    }

    // FIREFLY-0003: Refine device type from info response
    // CH340 (ESP8266-OLED) and CH9102 (ESP32-TDisplay) share VID 0x1a86.
    // The info response contains the firmware identifier that distinguishes them.
    let refined = FireflyDeviceType::refine_from_info(detected.device_type, &response);
    if refined != detected.device_type {
        tracing::info!(
            port = %detected.port_name,
            old = %detected.device_type,
            new = %refined,
            "Refined device type from info response"
        );
        // Update the detected device (the caller holds a mutable reference via try_connect)
        // We return the response; caller reads the refined type from the response too.
    }

    Ok(response)
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
        // CH340/CH9102 share VID — defaults to ESP8266, refined by info response
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
    fn test_refine_from_info_tdisplay() {
        // FIREFLY-0003: CH9102 returns firefly-tdisplay in info response
        let refined = FireflyDeviceType::refine_from_info(
            FireflyDeviceType::Esp8266Oled,
            "OK,firefly-tdisplay,esp32,135x240",
        );
        assert_eq!(refined, FireflyDeviceType::Esp32TDisplay);
    }

    #[test]
    fn test_refine_from_info_oled_unchanged() {
        let refined = FireflyDeviceType::refine_from_info(
            FireflyDeviceType::Esp8266Oled,
            "OK,firefly-oled,esp8266,128x64",
        );
        assert_eq!(refined, FireflyDeviceType::Esp8266Oled);
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
