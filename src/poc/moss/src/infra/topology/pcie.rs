//! PCIe device enumeration with link negotiation details (ARCH-0014).
//!
//! - Linux: sysfs `/sys/bus/pci/devices/*/` for link speed/width
//! - Windows: `cfgmgr32` `DEVPKEY_PciDevice_*` for link speed/width

use anyhow::Result;
use garden_common::types::hardware_topology::{PcieDevice, ThunderboltPort};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use pci_ids::FromId;

/// Detect all PCIe devices with link negotiation details.
pub async fn detect_pcie_devices() -> Result<Vec<PcieDevice>> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(detect_pcie_linux).await?
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(detect_pcie_windows).await?
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(Vec::new())
    }
}

/// Extract Thunderbolt ports from detected PCIe devices.
///
/// Thunderbolt controllers are PCIe devices with known Intel device IDs.
/// We detect them during PCIe enumeration and extract port metadata.
pub fn extract_thunderbolt_ports(pcie_devices: &[PcieDevice]) -> Vec<ThunderboltPort> {
    pcie_devices
        .iter()
        .filter_map(thunderbolt_from_pcie)
        .collect()
}

fn thunderbolt_from_pcie(dev: &PcieDevice) -> Option<ThunderboltPort> {
    let id = dev.device_id.to_lowercase();

    // Intel Thunderbolt controller PCI IDs → version mapping
    let (version, kind) = match id.as_str() {
        // Thunderbolt 1 — Light Ridge
        "8086:1513" | "8086:151a" | "8086:151b" => (1, "thunderbolt"),
        // Thunderbolt 2 — Falcon Ridge
        "8086:156c" | "8086:156d" => (2, "thunderbolt"),
        // Thunderbolt 3 — Alpine Ridge
        "8086:15d2" | "8086:15d9" | "8086:15da" => (3, "thunderbolt"),
        // Thunderbolt 3 — Titan Ridge
        "8086:15e7" | "8086:15ea" | "8086:15eb" => (3, "thunderbolt"),
        // Thunderbolt 4 / USB4 — Maple Ridge
        "8086:9a1b" | "8086:9a1d" | "8086:9a1f" => (4, "usb4"),
        // USB4 v2 — Barlow Ridge
        "8086:a73e" | "8086:a73f" => (5, "usb4"),
        _ => return None,
    };

    let bandwidth_gbps = match version {
        1 | 2 => 20.0,
        3 => 40.0,
        4 => 40.0,
        5 => 80.0,
        _ => 0.0,
    };

    Some(ThunderboltPort {
        kind: kind.to_string(),
        version,
        bandwidth_gbps,
        controller_id: Some(id),
    })
}

// ── Linux ───────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn detect_pcie_linux() -> Result<Vec<PcieDevice>> {
    let pci_dir = std::path::Path::new("/sys/bus/pci/devices");
    if !pci_dir.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();

    for entry in std::fs::read_dir(pci_dir)? {
        let entry = entry?;
        let path = entry.path();
        let address = entry.file_name().to_string_lossy().to_string();

        let vendor_id = read_sysfs_hex(&path.join("vendor")).unwrap_or(0);
        let device_id = read_sysfs_hex(&path.join("device")).unwrap_or(0);
        let class_code = read_sysfs_hex(&path.join("class")).unwrap_or(0);

        if vendor_id == 0 && device_id == 0 {
            continue;
        }

        let max_width = read_sysfs_u8(&path.join("max_link_width")).unwrap_or(0);
        let cur_width = read_sysfs_u8(&path.join("current_link_width")).unwrap_or(0);
        let generation = parse_pcie_gen_from_speed(
            &read_sysfs_string(&path.join("max_link_speed")).unwrap_or_default(),
        );
        let cur_gen = parse_pcie_gen_from_speed(
            &read_sysfs_string(&path.join("current_link_speed")).unwrap_or_default(),
        );

        let driver = std::fs::read_link(path.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

        let class = pci_class_name(class_code >> 8);

        let vendor_name = pci_ids::Vendor::from_id(vendor_id as u16).map(|v| v.name().to_string());
        let device_name = pci_ids::Device::from_vid_pid(vendor_id as u16, device_id as u16)
            .map(|d| d.name().to_string());

        let effective_gen = if cur_gen > 0 { cur_gen } else { generation };
        let effective_width = if cur_width > 0 { cur_width } else { max_width };

        devices.push(PcieDevice {
            address,
            physical_width: max_width,
            negotiated_width: effective_width,
            generation: effective_gen,
            bandwidth_gbps: PcieDevice::compute_bandwidth(effective_width, effective_gen),
            class,
            device_name,
            vendor_name,
            device_id: format!("{:04x}:{:04x}", vendor_id, device_id),
            power_budget_w: None,
            driver,
        });
    }

    Ok(devices)
}

#[cfg(target_os = "linux")]
fn read_sysfs_hex(path: &std::path::Path) -> Option<u32> {
    let s = std::fs::read_to_string(path).ok()?;
    let trimmed = s.trim().trim_start_matches("0x");
    u32::from_str_radix(trimmed, 16).ok()
}

#[cfg(target_os = "linux")]
fn read_sysfs_u8(path: &std::path::Path) -> Option<u8> {
    let s = std::fs::read_to_string(path).ok()?;
    s.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn read_sysfs_string(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Parse PCIe generation from sysfs speed string.
/// Format: "8.0 GT/s PCIe" or "16.0 GT/s PCIe"
#[cfg(target_os = "linux")]
fn parse_pcie_gen_from_speed(speed: &str) -> u8 {
    if speed.contains("2.5") {
        1
    } else if speed.contains("5.0") {
        2
    } else if speed.contains("8.0") {
        3
    } else if speed.contains("16.0") {
        4
    } else if speed.contains("32.0") {
        5
    } else {
        0
    }
}

// ── Windows ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn detect_pcie_windows() -> Result<Vec<PcieDevice>> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let pci_key = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\PCI")?;

    let mut devices = Vec::new();

    for dev_key_name in pci_key.enum_keys().filter_map(|r| r.ok()) {
        let name_upper = dev_key_name.to_uppercase();
        let vendor_str = extract_between_win(&name_upper, "VEN_", "&");
        let device_str = extract_between_win(&name_upper, "DEV_", "&");

        let (vendor_id, device_id) = match (vendor_str, device_str) {
            (Some(v), Some(d)) => {
                let vid = u16::from_str_radix(v, 16).unwrap_or(0);
                let did = u16::from_str_radix(d, 16).unwrap_or(0);
                (vid, did)
            }
            _ => continue,
        };

        if vendor_id == 0 && device_id == 0 {
            continue;
        }

        let id_pair = format!("{:04x}:{:04x}", vendor_id, device_id);

        // Enumerate ALL instances of this device (multiple NICs, etc.)
        let dev_subkey = match pci_key.open_subkey(&dev_key_name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        for instance_name in dev_subkey.enum_keys().filter_map(|r| r.ok()) {
            let instance_key = match dev_subkey.open_subkey(&instance_name) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let raw_location: String = instance_key
                .get_value("LocationInformation")
                .unwrap_or_default();
            let driver: Option<String> = instance_key.get_value("Driver").ok();

            // Parse BDF from LocationInformation.
            // Format: "@System32\drivers\pci.sys,...;(bus,device,function)"
            let location = parse_bdf_from_location(&raw_location).unwrap_or(raw_location);

            let vendor_name = pci_ids::Vendor::from_id(vendor_id).map(|v| v.name().to_string());
            let device_name =
                pci_ids::Device::from_vid_pid(vendor_id, device_id).map(|d| d.name().to_string());

            // Link speed/width from DEVPKEY_PciDevice_* via cfgmgr32.
            let (max_width, cur_width, max_gen, cur_gen) =
                query_pcie_link_properties_win(&dev_key_name).unwrap_or((0, 0, 0, 0));

            let effective_gen = if cur_gen > 0 { cur_gen } else { max_gen };
            let effective_width = if cur_width > 0 { cur_width } else { max_width };

            // PCI class: try reading from registry, fall back to name heuristic
            let class = read_pci_class_from_registry(&dev_key_name).unwrap_or_else(|| {
                device_name
                    .as_deref()
                    .map(guess_pci_class_from_name)
                    .unwrap_or_else(|| "Unknown".to_string())
            });

            devices.push(PcieDevice {
                address: location,
                physical_width: max_width,
                negotiated_width: effective_width,
                generation: effective_gen,
                bandwidth_gbps: PcieDevice::compute_bandwidth(effective_width, effective_gen),
                class,
                device_name,
                vendor_name,
                device_id: id_pair.clone(),
                power_budget_w: None,
                driver,
            });
        }
    }

    Ok(devices)
}

/// Query PCIe link properties via cfgmgr32 DEVPKEY on Windows.
///
/// Returns (max_width, current_width, max_gen, current_gen).
/// Falls back to (0,0,0,0) if properties are unavailable.
///
/// Uses `CM_Locate_DevNodeW` + `CM_Get_DevNode_PropertyW` with hand-defined
/// `DEVPKEY_PciDevice_*` constants (not in the `windows` crate, sourced from WDK devpkey.h).
#[cfg(target_os = "windows")]
fn query_pcie_link_properties_win(dev_key: &str) -> Option<(u8, u8, u8, u8)> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows::Win32::Devices::Properties::DEVPROPKEY;
    use windows::core::GUID;

    // DEVPKEY_PciDevice_* from WDK devpkey.h
    // GUID: {3AB22E31-8264-4B4E-9AF5-A8D2D8E33E62}
    let pci_guid = GUID::from_u128(0x3AB22E31_8264_4B4E_9AF5_A8D2D8E33E62);

    let devpkey_max_link_speed = DEVPROPKEY {
        fmtid: pci_guid,
        pid: 8,
    };
    let devpkey_current_link_speed = DEVPROPKEY {
        fmtid: pci_guid,
        pid: 9,
    };
    let devpkey_max_link_width = DEVPROPKEY {
        fmtid: pci_guid,
        pid: 10,
    };
    let devpkey_current_link_width = DEVPROPKEY {
        fmtid: pci_guid,
        pid: 11,
    };

    // Find the first device instance under this registry key
    let instance_id = find_first_instance_id(dev_key)?;

    // Locate the device node
    let instance_wide: Vec<u16> = instance_id
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut dev_inst: u32 = 0;

    // SAFETY: instance_wide is null-terminated, dev_inst is a valid output pointer.
    let cr = unsafe {
        CM_Locate_DevNodeW(
            &mut dev_inst,
            windows::core::PCWSTR(instance_wide.as_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        )
    };
    if cr != CR_SUCCESS {
        return None;
    }

    let max_speed = read_devnode_u32(dev_inst, &devpkey_current_link_speed)
        .or_else(|| read_devnode_u32(dev_inst, &devpkey_max_link_speed));
    let cur_speed = read_devnode_u32(dev_inst, &devpkey_current_link_speed);
    let max_width = read_devnode_u32(dev_inst, &devpkey_max_link_width);
    let cur_width = read_devnode_u32(dev_inst, &devpkey_current_link_width);

    // Speed values from DEVPKEY: 1=Gen1, 2=Gen2, 3=Gen3, 4=Gen4, 5=Gen5
    let max_gen = max_speed.unwrap_or(0).min(255) as u8;
    let cur_gen = cur_speed.unwrap_or(0).min(255) as u8;
    let max_w = max_width.unwrap_or(0).min(255) as u8;
    let cur_w = cur_width.unwrap_or(0).min(255) as u8;

    if max_gen == 0 && cur_gen == 0 && max_w == 0 && cur_w == 0 {
        return None;
    }

    Some((max_w, cur_w, max_gen, cur_gen))
}

/// Read a u32 device node property via cfgmgr32.
#[cfg(target_os = "windows")]
fn read_devnode_u32(
    dev_inst: u32,
    property_key: &windows::Win32::Devices::Properties::DEVPROPKEY,
) -> Option<u32> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CR_SUCCESS,
    };
    use windows::Win32::Devices::Properties::DEVPROPTYPE;

    let mut prop_type = DEVPROPTYPE(0);
    let mut buffer = [0u8; 4];
    let mut buffer_size = 4u32;

    // SAFETY: buffer is 4 bytes, buffer_size is set to 4, property_key is valid.
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            dev_inst,
            property_key,
            &mut prop_type,
            Some(buffer.as_mut_ptr()),
            &mut buffer_size,
            0,
        )
    };

    if cr == CR_SUCCESS && buffer_size == 4 {
        Some(u32::from_le_bytes(buffer))
    } else {
        None
    }
}

/// Read PCI base class from the registry's CompatibleIDs.
///
/// CompatibleIDs contain entries like `PCI\CC_0300` (base class 03 = VGA).
#[cfg(target_os = "windows")]
fn read_pci_class_from_registry(dev_key: &str) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let full_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{}", dev_key);
    let dev_subkey = hklm.open_subkey(&full_path).ok()?;

    // Get first instance
    let instance_name = dev_subkey.enum_keys().filter_map(|r| r.ok()).next()?;
    let instance_key = dev_subkey.open_subkey(&instance_name).ok()?;

    // Read CompatibleIDs — multi-string containing PCI\CC_XXYY entries
    let compat_ids: Vec<String> = instance_key.get_value("CompatibleIDs").ok()?;
    for id in &compat_ids {
        let upper = id.to_uppercase();
        if let Some(cc_pos) = upper.find("CC_") {
            let cc_hex = &upper[cc_pos + 3..];
            if cc_hex.len() >= 2 {
                let base_class = u8::from_str_radix(&cc_hex[..2], 16).ok()?;
                return Some(pci_base_class_name(base_class).to_string());
            }
        }
    }

    None
}

/// Map PCI base class code to human-readable name.
#[cfg(target_os = "windows")]
fn pci_base_class_name(class: u8) -> &'static str {
    match class {
        0x00 => "Unclassified device",
        0x01 => "Mass storage controller",
        0x02 => "Network controller",
        0x03 => "VGA compatible controller",
        0x04 => "Multimedia controller",
        0x05 => "Memory controller",
        0x06 => "Bridge",
        0x07 => "Communication controller",
        0x08 => "System peripheral",
        0x09 => "Input device controller",
        0x0a => "Docking station",
        0x0b => "Processor",
        0x0c => "Serial bus controller",
        0x0d => "Wireless controller",
        0x0e => "Intelligent controller",
        0x0f => "Satellite communication",
        0x10 => "Encryption controller",
        0x11 => "Signal processing controller",
        0x12 => "Processing accelerator",
        0x13 => "Non-essential instrumentation",
        0xff => "Unassigned class",
        _ => "Unknown",
    }
}

/// Find the first device instance ID under a PCI registry key.
///
/// Registry layout: `HKLM\...\Enum\PCI\VEN_XXXX&DEV_XXXX&...\{instance}\`
/// We need the full instance path: `PCI\VEN_XXXX&DEV_XXXX&...\{instance}`
#[cfg(target_os = "windows")]
fn find_first_instance_id(dev_key: &str) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let full_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{}", dev_key);
    let dev_subkey = hklm.open_subkey(&full_path).ok()?;

    // First sub-key is the instance (e.g., "00000000")
    let instance_name = dev_subkey.enum_keys().filter_map(|r| r.ok()).next()?;

    // Full instance ID: PCI\VEN_XXXX&DEV_XXXX&...\instance
    Some(format!(r"PCI\{}\{}", dev_key, instance_name))
}

/// Parse BDF address from Windows LocationInformation string.
///
/// Input:  `@System32\drivers\pci.sys,#65536;PCI bus %1, device %2, function %3;(1,0,0)`
/// Output: `0000:01:00.0`
#[cfg(target_os = "windows")]
fn parse_bdf_from_location(location: &str) -> Option<String> {
    // Find the last parenthesized group: (bus,device,function)
    let start = location.rfind('(')?;
    let end = location.rfind(')')?;
    if end <= start {
        return None;
    }
    let inner = &location[start + 1..end];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let bus: u8 = parts[0].trim().parse().ok()?;
    let device: u8 = parts[1].trim().parse().ok()?;
    let function: u8 = parts[2].trim().parse().ok()?;
    Some(format!("0000:{:02x}:{:02x}.{}", bus, device, function))
}

#[cfg(target_os = "windows")]
fn extract_between_win<'a>(s: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = s.find(prefix)? + prefix.len();
    let rest = &s[start..];
    let end = rest.find(suffix).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[cfg(target_os = "windows")]
fn guess_pci_class_from_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("geforce") || lower.contains("radeon") || lower.contains("graphics") {
        "VGA compatible controller".to_string()
    } else if lower.contains("ethernet") || lower.contains("network") || lower.contains("wi-fi") {
        "Network controller".to_string()
    } else if lower.contains("nvme") || lower.contains("ssd") || lower.contains("ahci") {
        "Mass storage controller".to_string()
    } else if lower.contains("usb") || lower.contains("xhci") {
        "USB controller".to_string()
    } else if lower.contains("audio") || lower.contains("hda") {
        "Audio device".to_string()
    } else {
        "Other".to_string()
    }
}

// ── Shared ──────────────────────────────────────────────────────────────

/// Map PCI class code (top 16 bits) to human-readable name.
#[cfg(target_os = "linux")]
fn pci_class_name(class_code: u32) -> String {
    match (class_code >> 8) as u8 {
        0x00 => "Unclassified device",
        0x01 => "Mass storage controller",
        0x02 => "Network controller",
        0x03 => "VGA compatible controller",
        0x04 => "Multimedia controller",
        0x05 => "Memory controller",
        0x06 => "Bridge",
        0x07 => "Communication controller",
        0x08 => "System peripheral",
        0x09 => "Input device controller",
        0x0a => "Docking station",
        0x0b => "Processor",
        0x0c => "Serial bus controller",
        0x0d => "Wireless controller",
        0x0e => "Intelligent controller",
        0x0f => "Satellite communication",
        0x10 => "Encryption controller",
        0x11 => "Signal processing controller",
        0x12 => "Processing accelerator",
        0x13 => "Non-essential instrumentation",
        0xff => "Unassigned class",
        _ => "Unknown",
    }
    .to_string()
}
