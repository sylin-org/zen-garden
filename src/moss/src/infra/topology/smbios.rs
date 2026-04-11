//! SMBIOS/DMI detection — system identity, M.2 slot inventory, chassis type.
//!
//! Uses `smbios-lib` to parse raw SMBIOS tables on both Linux and Windows.
//! - Linux: reads `/sys/firmware/dmi/tables/`
//! - Windows: `GetSystemFirmwareTable(RSMB, 0, ...)`
//!
//! Both paths converge on the same parser — the only platform difference is
//! how the raw bytes are obtained.

use anyhow::Result;
use garden_common::types::hardware_topology::{M2Slot, MemorySlot, MemoryTopology, SystemIdentity};

/// Combined SMBIOS detection result.
pub struct SmbiosResult {
    pub identity: SystemIdentity,
    pub m2_slots: Vec<M2Slot>,
    pub memory: MemoryTopology,
}

/// Detect system identity and M.2 slot inventory from SMBIOS tables.
pub async fn detect_smbios() -> Result<SmbiosResult> {
    tokio::task::spawn_blocking(detect_smbios_blocking).await?
}

fn detect_smbios_blocking() -> Result<SmbiosResult> {
    let smbios = smbioslib::table_load_from_device()?;

    let identity = parse_identity(&smbios);
    let m2_slots = parse_m2_slots(&smbios);
    let memory = parse_memory_slots(&smbios);

    Ok(SmbiosResult {
        identity,
        m2_slots,
        memory,
    })
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
        board_manufacturer: None,
        board_product: None,
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
            if !serial.is_empty()
                && serial != "Default string"
                && serial != "To Be Filled By O.E.M."
            {
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

    // Type 2: Baseboard Information
    for board in smbios.collect::<smbioslib::SMBiosBaseboardInformation>() {
        if let Some(mfr) = board.manufacturer().ok() {
            if !mfr.is_empty() && mfr != "Default string" && mfr != "To Be Filled By O.E.M." {
                identity.board_manufacturer = Some(mfr);
            }
        }
        if let Some(product) = board.product().ok() {
            if !product.is_empty()
                && product != "Default string"
                && product != "To Be Filled By O.E.M."
            {
                identity.board_product = Some(product);
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
                if !serial.is_empty()
                    && serial != "Default string"
                    && serial != "To Be Filled By O.E.M."
                {
                    identity.serial = Some(serial);
                }
            }
        }
    }

    identity
}

fn parse_memory_slots(smbios: &smbioslib::SMBiosData) -> MemoryTopology {
    let mut slots = Vec::new();

    for device in smbios.collect::<smbioslib::SMBiosMemoryDevice>() {
        let locator = device.device_locator().ok().unwrap_or_default();

        // Skip entries with empty locators (some BIOS report phantom slots)
        if locator.is_empty() {
            continue;
        }

        // Resolve size in MB — handle both standard and extended size fields
        let size_mb = match device.size() {
            Some(smbioslib::MemorySize::Megabytes(mb)) => Some(mb as u64),
            Some(smbioslib::MemorySize::Kilobytes(kb)) => Some(kb as u64 / 1024),
            Some(smbioslib::MemorySize::SeeExtendedSize) => match device.extended_size() {
                Some(smbioslib::MemorySizeExtended::Megabytes(mb)) => Some(mb as u64),
                _ => None,
            },
            _ => None, // NotInstalled, Unknown, or absent
        };

        let populated = size_mb.map(|s| s > 0).unwrap_or(false);

        // Map MemoryDeviceType enum to human-readable string
        let memory_type = if populated {
            device
                .memory_type()
                .map(|t| memory_device_type_name(&t.value).to_string())
        } else {
            None
        };

        // Map MemoryFormFactor enum to human-readable string
        let form_factor = if populated {
            device
                .form_factor()
                .map(|f| memory_form_factor_name(&f.value).to_string())
        } else {
            None
        };

        // Prefer configured speed; fall back to max speed; handle extended fields
        let speed_mts = if populated {
            resolve_memory_speed(
                device.configured_memory_speed(),
                device.extended_configured_memory_speed(),
            )
            .or_else(|| resolve_memory_speed(device.speed(), device.extended_speed()))
        } else {
            None
        };

        let manufacturer = device.manufacturer().ok().filter(|m| {
            !m.is_empty()
                && m != "Unknown"
                && m != "Not Specified"
                && m != "Default string"
                && m != "To Be Filled By O.E.M."
        });

        slots.push(MemorySlot {
            locator,
            populated,
            size_mb: if populated { size_mb } else { None },
            memory_type,
            form_factor,
            speed_mts,
            manufacturer: if populated { manufacturer } else { None },
        });
    }

    MemoryTopology { slots }
}

/// Resolve speed from the standard field and its extended counterpart.
fn resolve_memory_speed(
    standard: Option<smbioslib::MemorySpeed>,
    extended: Option<smbioslib::MemorySpeedExtended>,
) -> Option<u32> {
    match standard {
        Some(smbioslib::MemorySpeed::MTs(mts)) => Some(mts as u32),
        Some(smbioslib::MemorySpeed::SeeExtendedSpeed) => match extended {
            Some(smbioslib::MemorySpeedExtended::MTs(mts)) => Some(mts),
            _ => None,
        },
        _ => None,
    }
}

/// Map `MemoryDeviceType` enum to a human-readable string.
fn memory_device_type_name(t: &smbioslib::MemoryDeviceType) -> &'static str {
    use smbioslib::MemoryDeviceType::*;
    match t {
        Other => "Other",
        Unknown => "Unknown",
        Dram => "DRAM",
        Edram => "EDRAM",
        Vram => "VRAM",
        Sram => "SRAM",
        Ram => "RAM",
        Rom => "ROM",
        Flash => "Flash",
        Eeprom => "EEPROM",
        Feprom => "FEPROM",
        Eprom => "EPROM",
        Cdram => "CDRAM",
        ThreeDram => "3DRAM",
        Sdram => "SDRAM",
        Sgram => "SGRAM",
        Rdram => "RDRAM",
        Ddr => "DDR",
        Ddr2 => "DDR2",
        Ddr2Fbdimm => "DDR2 FB-DIMM",
        Ddr3 => "DDR3",
        Fbd2 => "FBD2",
        Ddr4 => "DDR4",
        Lpddr => "LPDDR",
        Lpddr2 => "LPDDR2",
        Lpddr3 => "LPDDR3",
        Lpddr4 => "LPDDR4",
        LogicalNonVolatileDevice => "Logical Non-Volatile",
        Hbm => "HBM",
        Hbm2 => "HBM2",
        Ddr5 => "DDR5",
        Lpddr5 => "LPDDR5",
        Hbm3 => "HBM3",
        None => "Unknown",
    }
}

/// Map `MemoryFormFactor` enum to a human-readable string.
fn memory_form_factor_name(f: &smbioslib::MemoryFormFactor) -> &'static str {
    use smbioslib::MemoryFormFactor::*;
    match f {
        Other => "Other",
        Unknown => "Unknown",
        Simm => "SIMM",
        Sip => "SIP",
        Chip => "Chip",
        Dip => "DIP",
        Zip => "ZIP",
        ProprietaryCard => "Proprietary Card",
        Dimm => "DIMM",
        Tsop => "TSOP",
        RowOfChips => "Row Of Chips",
        Rimm => "RIMM",
        Sodimm => "SODIMM",
        Srimm => "SRIMM",
        Fbdimm => "FB-DIMM",
        Die => "Die",
        None => "Unknown",
    }
}

fn parse_m2_slots(smbios: &smbioslib::SMBiosData) -> Vec<M2Slot> {
    let mut slots = Vec::new();

    // Type 9: System Slots
    for slot in smbios.collect::<smbioslib::SMBiosSystemSlot>() {
        let designation = slot.slot_designation().ok().unwrap_or_default();

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
