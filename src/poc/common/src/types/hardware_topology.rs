//! Hardware topology types — Tier 2 deep hardware probing (ARCH-0014).
//!
//! These types represent the exploratory hardware topology: PCIe devices,
//! M.2 slots, Thunderbolt ports, USB summary, network interfaces, and
//! firmware inventory. Collected in the background, cached across boots,
//! and delta-gated via a SHA-256 fingerprint of PCI device IDs.
//!
//! Tier 1 (HardwareCapabilities) gates offering compatibility.
//! Tier 2 (HardwareTopology) enables fleet planning and eGPU expansion.

use serde::{Deserialize, Serialize};

// ── Root ────────────────────────────────────────────────────────────────

/// Full capabilities response composing both tiers.
///
/// Returned by `GET /api/v1/stone/capabilities`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullCapabilities {
    /// Tier 1 — always populated, fast, gates offering compatibility.
    pub core: super::HardwareCapabilities,
    /// Tier 2 — `None` during first probe on a fresh install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topology: Option<HardwareTopology>,
}

/// Tier 2 hardware topology — deep, cached, background-probed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareTopology {
    /// SHA-256 of PCI device IDs — detects hardware changes (eGPU hot-plug).
    pub fingerprint: String,
    /// Detection logic version — detects code changes (new filters, subsystems).
    /// Bump `PROBE_VERSION` in `topology_probe.rs` when detection logic changes.
    #[serde(default)]
    pub probe_version: u32,
    /// ISO 8601 timestamp of last full probe completion.
    pub probed_at: String,
    /// Probe progress.
    pub status: TopologyStatus,
    /// System identity from SMBIOS (manufacturer, product, serial, BIOS).
    pub system: SystemIdentity,
    /// Expansion bus topology (PCIe, M.2, Thunderbolt, USB).
    pub expansion: Expansion,
    /// Network interfaces with link speed and type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network: Vec<NetworkInterface>,
    /// Firmware inventory across all components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub firmware: Vec<FirmwareComponent>,
    /// Memory slot topology from SMBIOS Type 17 (Memory Device).
    #[serde(default)]
    pub memory: MemoryTopology,
}

/// Topology probe progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TopologyStatus {
    /// First probe or refresh in progress.
    Probing,
    /// Some subsystems complete, others still running.
    Partial,
    /// All probes finished.
    Complete,
}

// ── System Identity ─────────────────────────────────────────────────────

/// System identity from SMBIOS tables.
///
/// `manufacturer` and `product` are also available in Tier 1
/// (`HardwareInventory`) for offering matching. Tier 2 enriches with
/// serial number, BIOS version, and chassis type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemIdentity {
    /// System manufacturer (SMBIOS Type 1). e.g., "HP", "Dell Inc."
    pub manufacturer: String,
    /// Product name (SMBIOS Type 1). e.g., "t630 Thin Client"
    pub product: String,
    /// Chassis serial number (SMBIOS Type 3), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// System UUID (SMBIOS Type 1), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// BIOS/UEFI version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bios_version: Option<String>,
    /// BIOS release date (ISO 8601 or vendor format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bios_date: Option<String>,
    /// Chassis type from SMBIOS Type 3.
    /// Values: "desktop", "mini-pc", "thin-client", "laptop", "server", "unknown".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chassis_type: Option<String>,
    /// Baseboard manufacturer (SMBIOS Type 2). e.g., "ASUSTeK COMPUTER INC."
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub board_manufacturer: Option<String>,
    /// Baseboard product name (SMBIOS Type 2). e.g., "PRIME Z690-P WIFI"
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub board_product: Option<String>,
}

// ── Expansion ───────────────────────────────────────────────────────────

/// Expansion bus topology — PCIe, M.2, Thunderbolt, USB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expansion {
    /// PCIe devices (populated slots with link negotiation details).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pcie: Vec<PcieDevice>,
    /// M.2 slots (from SMBIOS Type 9 — includes empty slots if BIOS reports them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub m2: Vec<M2Slot>,
    /// Thunderbolt / USB4 ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thunderbolt: Vec<ThunderboltPort>,
    /// USB port summary grouped by version.
    pub usb: UsbSummary,
}

/// A PCIe device with link negotiation details.
///
/// Both physical and negotiated widths are captured — a "x16 wired x8"
/// slot (common in compact machines) has `physical_width: 16, negotiated_width: 8`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieDevice {
    /// Bus:Device.Function address. e.g., "0000:01:00.0"
    pub address: String,
    /// Physical slot lane count (x1, x4, x8, x16).
    pub physical_width: u8,
    /// Actual negotiated lane count.
    pub negotiated_width: u8,
    /// PCIe generation (3, 4, 5).
    pub generation: u8,
    /// Computed bandwidth: negotiated_width * gen transfer rate (GT/s) * encoding.
    pub bandwidth_gbps: f32,
    /// PCI class description. e.g., "VGA compatible controller", "Network controller".
    pub class: String,
    /// Device name. e.g., "NVIDIA GeForce RTX 3060", "Intel I225-V".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Vendor name. e.g., "NVIDIA Corporation", "Intel Corporation".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_name: Option<String>,
    /// Vendor:Device ID pair. e.g., "10de:2684".
    pub device_id: String,
    /// Slot power budget in watts, if detectable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_budget_w: Option<u8>,
    /// Kernel driver in use. e.g., "nvidia", "amdgpu", "e1000e".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
}

/// An M.2 slot from SMBIOS Type 9 (System Slots).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct M2Slot {
    /// Slot designation from SMBIOS. e.g., "M2_1", "WLAN".
    pub designation: String,
    /// Key type. e.g., "M", "E", "A+E", "B+M".
    pub key: String,
    /// Current usage: true = occupied, false = available.
    pub in_use: bool,
    /// Occupant device name, if populated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occupant: Option<String>,
    /// PCIe lanes routed to this slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie_lanes: Option<u8>,
    /// Supported form factors. e.g., ["2230", "2242", "2280"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form_factors: Vec<String>,
}

/// A Thunderbolt or USB4 port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThunderboltPort {
    /// Port kind: "thunderbolt" or "usb4".
    pub kind: String,
    /// Protocol version (3, 4, 5).
    pub version: u8,
    /// Theoretical bandwidth in Gbps (32, 40, 80).
    pub bandwidth_gbps: f32,
    /// Controller chip device ID for identification. e.g., "8086:9a1b".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_id: Option<String>,
}

// ── USB ─────────────────────────────────────────────────────────────────

/// USB summary — port groups by version + connected devices.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsbSummary {
    /// Port groups by USB version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<UsbPortGroup>,
    /// Currently connected USB devices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connected_devices: Vec<UsbDevice>,
}

/// USB ports grouped by version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbPortGroup {
    /// USB version string. e.g., "2.0", "3.0", "3.2 Gen2", "4".
    pub version: String,
    /// Number of ports at this version.
    pub count: u8,
}

/// A connected USB device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDevice {
    /// Vendor name (resolved from VID, or raw VID if unknown).
    pub vendor: String,
    /// Product name (resolved from PID, or raw PID if unknown).
    pub product: String,
    /// USB version of the port it's connected to.
    pub bus_version: String,
}

// ── Network ─────────────────────────────────────────────────────────────

/// A network interface with hardware details.
///
/// Distinct from `InterfaceResources` (live throughput counters).
/// This captures static hardware properties: type, speed, MAC, firmware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Interface name. e.g., "eth0", "enp3s0", "Ethernet".
    pub name: String,
    /// Interface kind: "ethernet", "wifi", "thunderbolt", "virtual", "loopback".
    pub kind: String,
    /// Negotiated link speed in Mbps. `None` if down or unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<u32>,
    /// MAC address. e.g., "aa:bb:cc:dd:ee:ff".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// NIC firmware version (driver-dependent, may be `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    /// PCIe bus address if this is a PCI NIC. e.g., "0000:03:00.0".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie_address: Option<String>,
}

// ── Firmware ────────────────────────────────────────────────────────────

/// A firmware component (BIOS, SSD controller, NIC, Thunderbolt, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareComponent {
    /// Component type. e.g., "BIOS", "SSD", "NIC", "Thunderbolt", "GPU".
    pub component: String,
    /// Vendor. e.g., "HP", "Intel", "Samsung".
    pub vendor: String,
    /// Version string. e.g., "F.62", "1.2.3", "EDA7602Q".
    pub version: String,
    /// Release or install date (ISO 8601 or vendor format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Whether this component is updatable (fwupd on Linux, ESRT on Windows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updatable: Option<bool>,
    /// Device name for correlation. e.g., "Samsung 970 EVO Plus 1TB".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

// ── Memory ─────────────────────────────────────────────────────────────

/// Memory topology from SMBIOS Type 17 (Memory Device).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryTopology {
    /// Individual memory slots (populated and empty).
    pub slots: Vec<MemorySlot>,
}

/// A single memory slot (DIMM/SODIMM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySlot {
    /// Slot locator label. e.g., "DIMM_A1", "ChannelA-DIMM0".
    pub locator: String,
    /// Whether this slot has a module installed.
    pub populated: bool,
    /// Module size in MB. `None` if slot is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_mb: Option<u64>,
    /// Memory type. e.g., "DDR4", "DDR5", "DDR3", "LPDDR4".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
    /// Physical form factor. e.g., "DIMM", "SODIMM", "RowOfChips".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_factor: Option<String>,
    /// Configured speed in MT/s. e.g., 3200, 2400.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_mts: Option<u32>,
    /// Module manufacturer. e.g., "Samsung", "Micron".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
}

// ── Garden Inspection ───────────────────────────────────────────────

/// Result of a garden-wide hardware inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenInspection {
    pub inspected_at: String,
    pub summary: InspectionSummary,
    pub stones: Vec<StoneInspection>,
    pub unreachable: Vec<UnreachableStone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionSummary {
    pub total: usize,
    pub inspected: usize,
    pub unreachable: usize,
}

/// Full capabilities for a single stone in a garden inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInspection {
    pub name: String,
    pub id: String,
    pub endpoint: String,
    #[serde(flatten)]
    pub capabilities: FullCapabilities,
}

/// A stone that could not be reached during inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnreachableStone {
    pub name: String,
    pub endpoint: String,
    pub reason: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────

impl PcieDevice {
    /// Compute bandwidth in Gbps from negotiated width and generation.
    ///
    /// PCIe transfer rates (per lane, after encoding):
    /// - Gen 1: 0.25 GB/s = 2.0 Gbps
    /// - Gen 2: 0.50 GB/s = 4.0 Gbps
    /// - Gen 3: ~0.985 GB/s ≈ 7.88 Gbps (128b/130b encoding)
    /// - Gen 4: ~1.969 GB/s ≈ 15.75 Gbps
    /// - Gen 5: ~3.938 GB/s ≈ 31.51 Gbps
    pub fn compute_bandwidth(negotiated_width: u8, generation: u8) -> f32 {
        let per_lane_gbps = match generation {
            1 => 2.0,
            2 => 4.0,
            3 => 7.88,
            4 => 15.75,
            5 => 31.51,
            _ => 0.0,
        };
        per_lane_gbps * negotiated_width as f32
    }
}

impl HardwareTopology {
    /// Create an empty topology in probing state.
    pub fn probing(fingerprint: String, probe_version: u32) -> Self {
        Self {
            fingerprint,
            probe_version,
            probed_at: String::new(),
            status: TopologyStatus::Probing,
            system: SystemIdentity {
                manufacturer: String::new(),
                product: String::new(),
                serial: None,
                uuid: None,
                bios_version: None,
                bios_date: None,
                chassis_type: None,
                board_manufacturer: None,
                board_product: None,
            },
            expansion: Expansion {
                pcie: Vec::new(),
                m2: Vec::new(),
                thunderbolt: Vec::new(),
                usb: UsbSummary::default(),
            },
            network: Vec::new(),
            firmware: Vec::new(),
            memory: MemoryTopology::default(),
        }
    }
}

/// SMBIOS chassis type code to human-readable string.
///
/// See SMBIOS spec Table 17 — System Enclosure or Chassis Types.
pub fn chassis_type_name(code: u8) -> &'static str {
    match code {
        1 => "other",
        2 => "unknown",
        3..=7 => "desktop",
        8 | 9 | 10 | 14 | 31 => "laptop",
        11 => "handheld",
        13 => "all-in-one",
        15 | 16 => "mini-pc",     // space-saving, lunch-box
        17 | 23 | 28 => "server", // main server, rack mount, blade
        24 => "sealed-case",      // many thin clients report this
        30 => "tablet",
        35 => "mini-pc",  // SMBIOS 3.1+: Mini PC
        36 => "stick-pc", // SMBIOS 3.1+: Stick PC
        _ => "unknown",
    }
}
