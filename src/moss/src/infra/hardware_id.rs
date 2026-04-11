//! Hardware-based Stone ID Generation
//!
//! Generates a stable, hardware-derived identifier for the stone that persists
//! across OS reinstalls, hostname changes, and IP changes. Uses multiple fallback
//! strategies to ensure reliability across different OS versions and configurations.
//!
//! ## Strategy (in order of preference):
//!
//! ### Windows:
//! 1. WMI Motherboard UUID (most reliable, Win7+)
//! 2. Windows MachineGuid from Registry (Win Vista+)
//! 3. SMBIOS/DMI data via wmic (Win XP+)
//! 4. MAC address hash (universal fallback)
//! 5. Persisted random ID (last resort)
//!
//! ### Linux:
//! 1. /sys/class/dmi/id/product_uuid (most reliable, requires root)
//! 2. /etc/machine-id (systemd standard, most distros)
//! 3. /var/lib/dbus/machine-id (D-Bus standard, older systems)
//! 4. DMI/SMBIOS via dmidecode (requires root)
//! 5. MAC address hash (universal fallback)
//! 6. Persisted random ID (last resort)
//!
//! The generated ID is a GUIDv5 (namespace-based SHA-1) derived from hardware
//! characteristics, ensuring the same hardware always produces the same ID.

use anyhow::{Context, Result};
use std::path::PathBuf;
use uuid::Uuid;

/// Hardware ID namespace for GUIDv5 generation
/// This is a fixed UUID that namespaces all hardware-derived IDs
const HARDWARE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Generate a hardware-derived stone ID
///
/// Tries multiple methods in order of preference, falling back to less reliable
/// methods if better ones fail. Returns a GUIDv5 derived from hardware characteristics.
pub async fn generate_hardware_id() -> String {
    // Try platform-specific methods first (most reliable)
    if let Ok(hw_string) = get_platform_hardware_string().await {
        let guid = Uuid::new_v5(&HARDWARE_NAMESPACE, hw_string.as_bytes());
        tracing::info!(
            method = "platform_specific",
            "Generated hardware-based stone ID"
        );
        return guid.to_string();
    }

    // Fallback 1: Network MAC addresses (works everywhere)
    if let Ok(mac_string) = get_mac_address_string().await {
        let guid = Uuid::new_v5(&HARDWARE_NAMESPACE, mac_string.as_bytes());
        tracing::warn!(
            method = "mac_address",
            "Generated stone ID from MAC address (fallback)"
        );
        return guid.to_string();
    }

    // Fallback 2: Hostname-based (least reliable, but universal)
    let hostname = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown-host"))
        .to_string_lossy()
        .to_lowercase();
    let guid = Uuid::new_v5(&HARDWARE_NAMESPACE, hostname.as_bytes());
    tracing::warn!(
        method = "hostname",
        "Generated stone ID from hostname (weak fallback)"
    );
    guid.to_string()
}

/// Get platform-specific hardware identifier string
#[cfg(target_os = "windows")]
async fn get_platform_hardware_string() -> Result<String> {
    // Strategy 1: WMI Motherboard UUID (most reliable)
    if let Ok(uuid) = get_windows_motherboard_uuid().await {
        return Ok(uuid);
    }

    // Strategy 2: Windows MachineGuid (very reliable, always present)
    if let Ok(guid) = get_windows_machine_guid().await {
        return Ok(guid);
    }

    // Strategy 3: SMBIOS data via wmic
    if let Ok(serial) = get_windows_system_serial().await {
        return Ok(serial);
    }

    anyhow::bail!("All Windows hardware ID methods failed")
}

#[cfg(target_os = "linux")]
async fn get_platform_hardware_string() -> Result<String> {
    // Strategy 1: DMI product UUID (most reliable)
    if let Ok(uuid) = tokio::fs::read_to_string("/sys/class/dmi/id/product_uuid").await {
        let uuid = uuid.trim();
        if !uuid.is_empty() && uuid != "00000000-0000-0000-0000-000000000000" {
            return Ok(uuid.to_string());
        }
    }

    // Strategy 2: systemd machine-id (standard on modern systems)
    if let Ok(machine_id) = tokio::fs::read_to_string("/etc/machine-id").await {
        let machine_id = machine_id.trim();
        if !machine_id.is_empty() {
            return Ok(machine_id.to_string());
        }
    }

    // Strategy 3: D-Bus machine-id (older systems)
    if let Ok(machine_id) = tokio::fs::read_to_string("/var/lib/dbus/machine-id").await {
        let machine_id = machine_id.trim();
        if !machine_id.is_empty() {
            return Ok(machine_id.to_string());
        }
    }

    // Strategy 4: DMI/SMBIOS data (composite of multiple fields)
    if let Ok(hw_id) = get_linux_dmi_composite().await {
        return Ok(hw_id);
    }

    anyhow::bail!("All Linux hardware ID methods failed")
}

#[cfg(target_os = "macos")]
async fn get_platform_hardware_string() -> Result<String> {
    // macOS: Use hardware UUID from IOKit
    let output = tokio::process::Command::new("system_profiler")
        .arg("SPHardwareDataType")
        .output()
        .await
        .context("Failed to run system_profiler")?;

    if !output.status.success() {
        anyhow::bail!("system_profiler command failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("Hardware UUID:") {
            if let Some(uuid) = line.split(':').nth(1) {
                return Ok(uuid.trim().to_string());
            }
        }
    }

    anyhow::bail!("Hardware UUID not found in system_profiler output")
}

// ============================================================================
// Windows-specific methods
// ============================================================================

#[cfg(target_os = "windows")]
async fn get_windows_motherboard_uuid() -> Result<String> {
    // Use PowerShell to query WMI for motherboard UUID
    let output = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance -ClassName Win32_ComputerSystemProduct | Select-Object -ExpandProperty UUID",
        ])
        .output()
        .await
        .context("Failed to run PowerShell WMI query")?;

    if !output.status.success() {
        anyhow::bail!("PowerShell WMI query failed");
    }

    let uuid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uuid.is_empty() || uuid.contains("FFFFFFFF") {
        anyhow::bail!("Invalid motherboard UUID");
    }

    Ok(uuid)
}

#[cfg(target_os = "windows")]
async fn get_windows_machine_guid() -> Result<String> {
    crate::infra::platform::registry::get_machine_guid()
}

#[cfg(target_os = "windows")]
async fn get_windows_system_serial() -> Result<String> {
    // Use wmic to get system serial number
    let output = tokio::process::Command::new("wmic")
        .args(["bios", "get", "serialnumber"])
        .output()
        .await
        .context("Failed to run wmic")?;

    if !output.status.success() {
        anyhow::bail!("wmic command failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // Skip header
        let serial = line.trim();
        if !serial.is_empty() && serial != "SerialNumber" {
            return Ok(serial.to_string());
        }
    }

    anyhow::bail!("System serial number not found")
}

// ============================================================================
// Linux-specific methods
// ============================================================================

#[cfg(target_os = "linux")]
async fn get_linux_dmi_composite() -> Result<String> {
    // Build composite ID from multiple DMI fields
    let mut parts = Vec::new();

    // Product UUID
    if let Ok(uuid) = tokio::fs::read_to_string("/sys/class/dmi/id/product_uuid").await {
        let uuid = uuid.trim();
        if !uuid.is_empty() && uuid != "00000000-0000-0000-0000-000000000000" {
            parts.push(uuid.to_string());
        }
    }

    // Board serial
    if let Ok(serial) = tokio::fs::read_to_string("/sys/class/dmi/id/board_serial").await {
        let serial = serial.trim();
        if !serial.is_empty() && serial != "None" {
            parts.push(serial.to_string());
        }
    }

    // Product serial
    if let Ok(serial) = tokio::fs::read_to_string("/sys/class/dmi/id/product_serial").await {
        let serial = serial.trim();
        if !serial.is_empty() && serial != "None" {
            parts.push(serial.to_string());
        }
    }

    if parts.is_empty() {
        anyhow::bail!("No valid DMI data found");
    }

    Ok(parts.join(":"))
}

// ============================================================================
// Universal fallback methods
// ============================================================================

async fn get_mac_address_string() -> Result<String> {
    // Get all network interfaces and their MAC addresses
    let output = if cfg!(target_os = "windows") {
        tokio::process::Command::new("getmac")
            .arg("/FO")
            .arg("CSV")
            .arg("/NH")
            .output()
            .await
            .context("Failed to run getmac")?
    } else {
        tokio::process::Command::new("ip")
            .arg("link")
            .arg("show")
            .output()
            .await
            .context("Failed to run ip link")?
    };

    if !output.status.success() {
        anyhow::bail!("Network interface query failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut macs = Vec::new();

    if cfg!(target_os = "windows") {
        // Parse getmac CSV output: "MAC Address","Transport Name"
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if let Some(mac) = parts.first() {
                let mac = mac.trim().trim_matches('"').replace('-', ":");
                if is_valid_mac(&mac) {
                    macs.push(mac.to_lowercase());
                }
            }
        }
    } else {
        // Parse ip link output: look for "link/ether XX:XX:XX:XX:XX:XX"
        for line in stdout.lines() {
            if line.contains("link/ether") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(mac_idx) = parts.iter().position(|&s| s == "link/ether")
                    && let Some(mac) = parts.get(mac_idx + 1)
                    && is_valid_mac(mac)
                {
                    macs.push(mac.to_lowercase());
                }
            }
        }
    }

    // Filter out virtual/temporary MACs and sort for stability
    macs.retain(|mac| !is_virtual_mac(mac));
    macs.sort();

    if macs.is_empty() {
        anyhow::bail!("No valid MAC addresses found");
    }

    // Use the first (lowest) MAC address for stability
    Ok(macs[0].clone())
}

fn is_valid_mac(mac: &str) -> bool {
    // Basic validation: should be XX:XX:XX:XX:XX:XX format
    let parts: Vec<&str> = mac.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2)
}

fn is_virtual_mac(mac: &str) -> bool {
    // Filter out common virtual MAC prefixes
    let virtual_prefixes = [
        "00:00:00", // Null MAC
        "00:05:69", // VMware
        "00:0c:29", // VMware
        "00:50:56", // VMware
        "00:1c:14", // VMware
        "00:15:5d", // Hyper-V
        "08:00:27", // VirtualBox
        "52:54:00", // QEMU/KVM
        "02:00:00", // Locally administered (often virtual)
    ];

    let mac_lower = mac.to_lowercase();
    virtual_prefixes
        .iter()
        .any(|prefix| mac_lower.starts_with(prefix))
}

/// Get path where hardware ID is cached
fn hardware_id_cache_path() -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir()).join("hardware-id")
}

/// Load cached hardware ID if it exists
pub async fn load_cached_hardware_id() -> Option<String> {
    let path = hardware_id_cache_path();
    if let Ok(content) = tokio::fs::read_to_string(&path).await {
        let id = content.trim();
        if !id.is_empty() {
            tracing::debug!(path = ?path, "Loaded cached hardware ID");
            return Some(id.to_string());
        }
    }
    None
}

/// Save hardware ID to cache
pub async fn save_hardware_id_cache(id: &str) -> Result<()> {
    let path = hardware_id_cache_path();
    let dir = path.parent().context("Invalid cache path")?;
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::write(&path, id).await?;
    tracing::debug!(path = ?path, "Cached hardware ID");
    Ok(())
}

/// Check if this is the first run on Windows by checking hardware-id cache existence
///
/// Returns true if hardware-id cache does NOT exist (first boot).
/// This is used for Windows first-boot detection instead of a separate flag file.
#[cfg(target_os = "windows")]
pub fn is_first_run_windows() -> bool {
    let path = hardware_id_cache_path();
    !path.exists()
}

/// Get path where stone name is cached
///
/// This provides a reliable persistence mechanism for the generated stone name,
/// independent of the config file. Particularly important on Windows where
/// the config file might not be read correctly on subsequent boots.
fn stone_name_cache_path() -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir()).join("stone-name")
}

/// Load cached stone name if it exists
///
/// This is the authoritative source for the stone name on Windows.
/// Falls back to None if file doesn't exist or is empty.
pub fn load_cached_stone_name() -> Option<String> {
    let path = stone_name_cache_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let name = content.trim();
        if !name.is_empty() {
            tracing::debug!(path = ?path, name = %name, "Loaded cached stone name");
            return Some(name.to_string());
        }
    }
    None
}

/// Save stone name to cache
///
/// Called after generating a new stone name on first boot.
/// This ensures the name persists even if config file has issues.
pub async fn save_stone_name_cache(name: &str) -> Result<()> {
    let path = stone_name_cache_path();
    let dir = path.parent().context("Invalid cache path")?;
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::write(&path, name).await?;
    tracing::info!(path = ?path, name = %name, "Cached stone name");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_validation() {
        assert!(is_valid_mac("00:11:22:33:44:55"));
        assert!(is_valid_mac("aa:bb:cc:dd:ee:ff"));
        assert!(!is_valid_mac("00:11:22:33:44")); // Too short
        assert!(!is_valid_mac("00:11:22:33:44:55:66")); // Too long
        assert!(!is_valid_mac("invalid"));
    }

    #[test]
    fn test_virtual_mac_detection() {
        assert!(is_virtual_mac("00:05:69:12:34:56")); // VMware
        assert!(is_virtual_mac("00:15:5d:ab:cd:ef")); // Hyper-V
        assert!(is_virtual_mac("52:54:00:12:34:56")); // QEMU
        assert!(!is_virtual_mac("ac:de:48:00:11:22")); // Real MAC
    }

    #[test]
    fn test_guid_generation() {
        // Same input should always produce same GUID
        let hw_string = "test-hardware-id";
        let guid1 = Uuid::new_v5(&HARDWARE_NAMESPACE, hw_string.as_bytes());
        let guid2 = Uuid::new_v5(&HARDWARE_NAMESPACE, hw_string.as_bytes());
        assert_eq!(guid1, guid2);

        // Different inputs should produce different GUIDs
        let guid3 = Uuid::new_v5(&HARDWARE_NAMESPACE, "different-hardware".as_bytes());
        assert_ne!(guid1, guid3);
    }
}
