//! Network interface detection (ARCH-0014).
//!
//! Captures static hardware properties: interface type, link speed, MAC,
//! firmware version. Distinct from `InterfaceMetrics` (live throughput).
//!
//! - Linux: sysfs `/sys/class/net/*/` + `ethtool`
//! - Windows: `GetAdaptersAddresses` + WMI for speed/firmware

use anyhow::Result;
use garden_common::types::hardware_topology::NetworkInterface;

/// Detect network interfaces with hardware details.
pub async fn detect_network_interfaces() -> Result<Vec<NetworkInterface>> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(detect_network_linux).await?
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(detect_network_windows).await?
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(Vec::new())
    }
}

// ── Linux ───────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn detect_network_linux() -> Result<Vec<NetworkInterface>> {
    let net_dir = std::path::Path::new("/sys/class/net");
    if !net_dir.exists() {
        return Ok(Vec::new());
    }

    let mut interfaces = Vec::new();

    for entry in std::fs::read_dir(net_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();

        // Skip loopback and virtual interfaces — only physical hardware
        if name == "lo" {
            continue;
        }

        // Determine kind from type file or driver
        let kind = detect_interface_kind_linux(&path, &name);

        // Filter: only physical interfaces.
        // A physical NIC has a `device` symlink in sysfs pointing to its PCI/USB
        // bus device. Virtual interfaces (docker0, veth*, br-*, tun*, tap*) do not.
        if !path.join("device").exists() {
            continue;
        }

        // Link speed (Mbps) — only valid when link is up
        let speed_mbps = std::fs::read_to_string(path.join("speed"))
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .and_then(|s| if s > 0 { Some(s as u32) } else { None });

        // MAC address
        let mac = std::fs::read_to_string(path.join("address"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "00:00:00:00:00:00");

        // Firmware version via ethtool -i (best-effort, requires privileges)
        let firmware_version = get_nic_firmware_linux(&name);

        // PCIe address from device symlink
        let pcie_address = std::fs::read_link(path.join("device"))
            .ok()
            .and_then(|link| {
                link.file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .filter(|addr| addr.contains(':'));

        interfaces.push(NetworkInterface {
            name,
            kind,
            speed_mbps,
            mac,
            firmware_version,
            pcie_address,
        });
    }

    Ok(interfaces)
}

#[cfg(target_os = "linux")]
fn detect_interface_kind_linux(path: &std::path::Path, name: &str) -> String {
    // Check type file (1 = ethernet, 801 = wifi, 772 = loopback)
    if let Ok(type_str) = std::fs::read_to_string(path.join("type")) {
        match type_str.trim() {
            "1" => {
                // Could be ethernet or wifi — check for wireless directory
                if path.join("wireless").exists() || path.join("phy80211").exists() {
                    return "wifi".to_string();
                }
                return "ethernet".to_string();
            }
            "801" => return "wifi".to_string(),
            "772" => return "loopback".to_string(),
            _ => {}
        }
    }

    // Fallback: name heuristics
    if name.starts_with("wl") || name.starts_with("wlan") {
        "wifi".to_string()
    } else if name.starts_with("eth") || name.starts_with("en") {
        "ethernet".to_string()
    } else if name.starts_with("docker") || name.starts_with("veth") || name.starts_with("br-") {
        "virtual".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(target_os = "linux")]
fn get_nic_firmware_linux(iface: &str) -> Option<String> {
    let output = std::process::Command::new("ethtool")
        .args(["-i", iface])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("firmware-version:") {
            let fw = value.trim().to_string();
            if !fw.is_empty() && fw != "N/A" {
                return Some(fw);
            }
        }
    }

    None
}

// ── Windows ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn detect_network_windows() -> Result<Vec<NetworkInterface>> {
    let mut interfaces = Vec::new();

    // Win32_NetworkAdapter (root\CIMV2) — reliable across all Windows versions.
    // PhysicalAdapter filters out Hyper-V, Docker, VPN virtual adapters.
    // Fields: Name (description), NetConnectionID (friendly name), Speed (bps),
    //         MACAddress ("AA:BB:CC:DD:EE:FF"), AdapterType ("Ethernet 802.3").
    let wmi_con = wmi::COMLibrary::new()
        .and_then(|lib| wmi::WMIConnection::new(lib))?;

    #[derive(serde::Deserialize)]
    #[serde(rename = "Win32_NetworkAdapter")]
    #[serde(rename_all = "PascalCase")]
    struct NetworkAdapter {
        /// Hardware description. e.g., "Realtek Gaming 2.5GbE Family Controller"
        name: Option<String>,
        /// User-visible connection name. e.g., "Ethernet", "Wi-Fi"
        net_connection_id: Option<String>,
        /// Link speed in bits/sec. e.g., 1000000000 for 1 Gbps.
        speed: Option<u64>,
        /// MAC address. e.g., "50:EB:F6:B4:19:BA"
        #[serde(rename = "MACAddress")]
        mac_address: Option<String>,
        /// True for physical hardware, false for virtual.
        physical_adapter: Option<bool>,
    }

    let adapters: Vec<NetworkAdapter> = wmi_con.query()?;

    for adapter in adapters {
        // Only physical adapters
        if !adapter.physical_adapter.unwrap_or(false) {
            continue;
        }

        let friendly_name = adapter.net_connection_id.unwrap_or_default();
        let description = adapter.name.unwrap_or_default();
        if friendly_name.is_empty() && description.is_empty() {
            continue;
        }

        // Determine kind from adapter type and name
        let kind = if description.to_lowercase().contains("wi-fi")
            || description.to_lowercase().contains("wireless")
            || description.to_lowercase().contains("802.11")
        {
            "wifi".to_string()
        } else if description.to_lowercase().contains("bluetooth") {
            // Skip Bluetooth PAN adapters — not a network interface
            continue;
        } else {
            "ethernet".to_string()
        };

        // Speed: filter out bogus values (Wi-Fi sometimes reports i64::MAX)
        let speed_mbps = adapter
            .speed
            .filter(|&s| s > 0 && s < 1_000_000_000_000) // < 1 Tbps sanity check
            .map(|bps| (bps / 1_000_000) as u32);

        let mac = adapter
            .mac_address
            .map(|m| m.replace('-', ":").to_lowercase())
            .filter(|m| !m.is_empty() && m != "00:00:00:00:00:00");

        // Use friendly name ("Ethernet", "Wi-Fi") as the interface name,
        // fall back to hardware description
        let name = if !friendly_name.is_empty() {
            friendly_name
        } else {
            description
        };

        interfaces.push(NetworkInterface {
            name,
            kind,
            speed_mbps,
            mac,
            firmware_version: None,
            pcie_address: None,
        });
    }

    Ok(interfaces)
}
