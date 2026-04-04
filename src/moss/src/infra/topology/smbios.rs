//! SMBIOS/DMI detection — system identity, M.2 slot inventory, chassis type.
//!
//! Uses `smbios-lib` to parse raw SMBIOS tables on both Linux and Windows.
//! - Linux: reads `/sys/firmware/dmi/tables/`
//! - Windows: `GetSystemFirmwareTable(RSMB, 0, ...)`
//!
//! Both paths converge on the same parser — the only platform difference is
//! how the raw bytes are obtained.

use anyhow::Result;
use garden_common::types::hardware_topology::{M2Slot, SystemIdentity};

/// Combined SMBIOS detection result.
pub struct SmbiosResult {
    pub identity: SystemIdentity,
    pub m2_slots: Vec<M2Slot>,
}

/// Detect system identity and M.2 slot inventory from SMBIOS tables.
pub async fn detect_smbios() -> Result<SmbiosResult> {
    tokio::task::spawn_blocking(detect_smbios_blocking).await?
}

fn detect_smbios_blocking() -> Result<SmbiosResult> {
    let smbios = smbioslib::table_load_from_device()?;

    let identity = parse_identity(&smbios);
    let m2_slots = parse_m2_slots(&smbios);

    Ok(SmbiosResult { identity, m2_slots })
}

fn parse_identity(smbios: &smbioslib::SMBiosData) -> SystemIdentity {
    use garden_common::types::hardware_topology::chassis_type_name;

    let mut identity = SystemIdentity {
        manufacturer: String::new(),
        product: String::new(),
        serial: None,
        uuid: None,
        bios_version: None,
        bios_date: None,
        chassis_type: None,
    };

    // Type 1: System Information
    for sys in smbios.collect::<smbioslib::SMBiosSystemInformation>() {
        if let Some(mfr) = sys.manufacturer().ok() {
            identity.manufacturer = mfr;
        }
        if let Some(product) = sys.product_name().ok() {
            identity.product = product;
        }
        if let Some(serial) = sys.serial_number().ok() {
            if !serial.is_empty() && serial != "Default string" && serial != "To Be Filled By O.E.M." {
                identity.serial = Some(serial);
            }
        }
        if let Some(uuid) = sys.uuid() {
            let formatted = format!("{:?}", uuid);
            if !formatted.is_empty() {
                identity.uuid = Some(formatted);
            }
        }
    }

    // Type 0: BIOS Information
    for bios in smbios.collect::<smbioslib::SMBiosInformation>() {
        if let Some(version) = bios.version().ok() {
            identity.bios_version = Some(version);
        }
        if let Some(date) = bios.release_date().ok() {
            identity.bios_date = Some(date);
        }
    }

    // Type 3: Chassis Information
    for chassis in smbios.collect::<smbioslib::SMBiosSystemChassisInformation>() {
        if let Some(chassis_type) = chassis.chassis_type() {
            identity.chassis_type = Some(chassis_type_name(chassis_type.raw).to_string());
        }
        // Prefer chassis serial if system serial was empty
        if identity.serial.is_none() {
            if let Some(serial) = chassis.serial_number().ok() {
                if !serial.is_empty() && serial != "Default string" && serial != "To Be Filled By O.E.M." {
                    identity.serial = Some(serial);
                }
            }
        }
    }

    identity
}

fn parse_m2_slots(smbios: &smbioslib::SMBiosData) -> Vec<M2Slot> {
    let mut slots = Vec::new();

    // Type 9: System Slots
    for slot in smbios.collect::<smbioslib::SMBiosSystemSlot>() {
        let designation = slot
            .slot_designation()
            .ok()
            .unwrap_or_default();

        // Filter for M.2 slots — look for M.2 in designation or slot type
        let designation_lower = designation.to_lowercase();
        let is_m2 = designation_lower.contains("m.2")
            || designation_lower.contains("m2")
            || designation_lower.contains("ngff")
            || designation_lower.contains("wlan")
            || designation_lower.contains("wifi");

        if !is_m2 {
            continue;
        }

        // Determine key type from designation heuristics
        let key = if designation_lower.contains("key e")
            || designation_lower.contains("wlan")
            || designation_lower.contains("wifi")
        {
            "E".to_string()
        } else if designation_lower.contains("key m") || designation_lower.contains("nvme") {
            "M".to_string()
        } else if designation_lower.contains("key b") {
            "B+M".to_string()
        } else {
            // Default: M key for generic M.2 slots
            "M".to_string()
        };

        let in_use = slot
            .current_usage()
            .map(|u| matches!(u.value, smbioslib::SlotCurrentUsage::InUse))
            .unwrap_or(false);

        slots.push(M2Slot {
            designation: designation.clone(),
            key,
            in_use,
            occupant: None, // Correlated later with PCI/NVMe devices
            pcie_lanes: None,
            form_factors: Vec::new(),
        });
    }

    slots
}
