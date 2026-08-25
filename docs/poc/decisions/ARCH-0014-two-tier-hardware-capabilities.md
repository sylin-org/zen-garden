---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-03
canonical: true
---

# ARCH-0014: Two-Tier Hardware Capabilities — Core Detection and Exploratory Topology

**Date**: 2026-04-03
**Status**: Accepted
**Depends on**: [ARCH-0012](ARCH-0012-typed-stone-api-client.md) (typed StoneApi client)

## Context

Moss detects hardware capabilities at startup through a 3-phase progressive
pipeline: CPU flags, memory, GPUs + runtimes, disk type, DMI/SMBIOS identity.
These capabilities gate offering compatibility (`requires: avx2`,
`requires: cuda`) and drive service placement decisions. The pipeline is
optimised for speed — Tier 1 data is available within seconds of boot.

Fleet planning requires a different class of hardware knowledge that Moss does
not capture today: PCIe slot topology (lane width, generation, negotiated speed,
occupants), M.2 slot inventory (key type, occupant), Thunderbolt/OCuLink/USB4
presence, USB port map, network interface capabilities, and component firmware
versions. This information is essential for:

- **eGPU expansion planning** — determining which stones can accept external
  GPUs, via which bus, at what bandwidth penalty.
- **Fleet discrepancy detection** — discovering that two nominally identical
  HP t630s have different BIOS versions, RAM configurations, or NIC firmware.
- **Connectivity auditing** — understanding available bandwidth and expansion
  headroom across the garden.

This exploratory data is expensive to collect (lspci deep probes, WMI
enumeration, dmidecode, fwupdmgr) and rarely changes. It should not block
startup, should not delay offering compatibility checks, and should be cached
aggressively.

## Decision

### Two-Tier Separation

Hardware capabilities are split into two tiers with distinct lifecycles:

**Tier 1 — Core** (existing, unchanged): CPU flags, cores, threads, memory,
GPUs + runtimes, disk type, system manufacturer/product, AI capabilities
summary. Fast to detect, always fresh, gates offering compatibility. The
existing 3-phase progressive detection pipeline continues to own this data.
Struct: `HardwareCapabilities` (no changes).

**Tier 2 — Topology** (new): PCIe devices, M.2 slots, Thunderbolt ports, USB
summary, network interfaces, firmware inventory, system serial number. Expensive
to collect, cached across boots, re-probed only when hardware changes or on
explicit request. Struct: `HardwareTopology`.

The two tiers are composed into a single `FullCapabilities` response at the API
level but are independently addressable.

### Endpoint Structure

```
GET  /api/v1/stone/capabilities              → FullCapabilities (Tier 1 + Tier 2)
GET  /api/v1/stone/capabilities/core         → HardwareCapabilities (Tier 1)
GET  /api/v1/stone/capabilities/topology     → HardwareTopology (Tier 2, null while probing)
POST /api/v1/stone/capabilities/refresh      → 202 Accepted, triggers immediate re-probe
```

The root `/capabilities` endpoint returns both tiers composed. `/core` returns
the fast, always-available Tier 1 data (identical to the previous
`/capabilities` response — backwards compatible). `/topology` returns the
exploratory Tier 2 data, which may be `null` during the first probe after a
fresh install. `/refresh` invalidates the cache and kicks a full re-probe
immediately, returning 202 with a job reference.

The existing `capabilities()` family in `StoneApi` (ARCH-0012) is extended:

```rust
api.capabilities().full()       // GET /capabilities
api.capabilities().core()       // GET /capabilities/core
api.capabilities().topology()   // GET /capabilities/topology
api.capabilities().refresh()    // POST /capabilities/refresh
```

### Garden Aggregation

A new garden endpoint aggregates topology across all stones:

```
GET /api/v1/garden/capabilities → Vec<FullCapabilities>
```

A new rake command (`rake garden inspect`) hits this endpoint and renders a
fleet-wide matrix view, grouping stones by `system_manufacturer + system_product`
and highlighting discrepancies within each group (RAM differences, firmware
version mismatches, missing expansion slots, etc.).

### Delta-Gated Reprobing

Tier 2 data is cached in `{data_dir}/hardware-topology.json`. The cache includes
a SHA-256 fingerprint derived from a quick probe (Linux: `lspci -n` device IDs;
Windows: PCI registry key enumeration). On boot:

1. Load cached `hardware-topology.json` (instant, serves stale-but-valid data).
2. Compute quick fingerprint (~2ms).
3. Compare to cached fingerprint.
4. **Match** → serve cache, schedule no re-probe.
5. **Mismatch** → schedule full re-probe in background.

As a safety net, a periodic re-probe runs every 24 hours via the caretaking
sweep cycle, regardless of fingerprint match. This catches changes invisible to
the fingerprint (firmware updates altering negotiated link widths, for example).

The `POST /capabilities/refresh` endpoint bypasses the fingerprint check and
forces an immediate full re-probe.

### Tier 2 Data Model

```rust
pub struct HardwareTopology {
    pub fingerprint: String,
    pub probed_at: String,             // ISO 8601 timestamp
    pub status: TopologyStatus,
    pub system: SystemIdentity,
    pub expansion: Expansion,
    pub network: Vec<NetworkInterface>,
    pub firmware: Vec<FirmwareComponent>,
}

pub enum TopologyStatus {
    Probing,   // first probe or refresh in progress
    Partial,   // some subsystems complete
    Complete,  // all probes finished
}

pub struct SystemIdentity {
    pub manufacturer: String,          // "HP Inc."
    pub product: String,               // "t630 Thin Client"
    pub serial: Option<String>,        // chassis serial, if available
    pub bios_version: Option<String>,  // from DMI/SMBIOS
    pub bios_date: Option<String>,     // "2024-03-15"
}

pub struct Expansion {
    pub pcie: Vec<PcieDevice>,
    pub m2: Vec<M2Slot>,
    pub thunderbolt: Vec<ThunderboltPort>,
    pub usb: UsbSummary,
}

pub struct PcieDevice {
    pub address: String,               // "0000:01:00.0" (BDF notation)
    pub physical_width: u8,            // physical slot lanes (x1, x4, x8, x16)
    pub negotiated_width: u8,          // actual negotiated (x16 wired x8 → 8)
    pub generation: u8,                // PCIe gen (3, 4, 5)
    pub bandwidth_gbps: f32,           // computed: width × gen transfer rate
    pub class: String,                 // "VGA compatible controller", "Network controller"
    pub occupant: Option<String>,      // "NVIDIA GeForce RTX 3060" or None if empty
    pub power_budget_w: Option<u8>,    // slot power delivery if detectable
    pub driver: Option<String>,        // "nvidia", "amdgpu", "e1000e"
}

pub struct M2Slot {
    pub key: String,                   // "M", "E", "A+E", "B+M"
    pub occupant: Option<String>,      // "Samsung 970 EVO Plus" or "Intel Wi-Fi 6 AX200"
    pub pcie_lanes: Option<u8>,        // lanes routed to this slot
    pub form_factors: Vec<String>,     // ["2230", "2242", "2280"]
}

pub struct ThunderboltPort {
    pub version: u8,                   // 3, 4, 5
    pub bandwidth_gbps: f32,           // 32, 40, 80
}

pub struct UsbSummary {
    pub ports: Vec<UsbPortGroup>,
    pub connected_devices: Vec<UsbDevice>,
}

pub struct UsbPortGroup {
    pub version: String,               // "2.0", "3.0", "3.2 Gen2", "4"
    pub count: u8,
}

pub struct UsbDevice {
    pub vendor: String,
    pub product: String,
    pub bus: String,                   // USB version of the port it's on
}

pub struct NetworkInterface {
    pub name: String,                  // "eth0", "enp3s0", "Ethernet"
    pub kind: String,                  // "ethernet", "wifi", "thunderbolt"
    pub speed_mbps: Option<u32>,       // negotiated link speed
    pub mac: Option<String>,
    pub firmware_version: Option<String>,
}

pub struct FirmwareComponent {
    pub component: String,             // "BIOS", "NIC", "SSD Controller", "Thunderbolt"
    pub vendor: String,                // "HP", "Intel", "Samsung"
    pub version: String,               // "F.62", "1.2.3"
    pub date: Option<String>,          // "2024-03-15"
    pub updatable: Option<bool>,       // fwupd reports this
}
```

### Composed Response

```rust
pub struct FullCapabilities {
    pub core: HardwareCapabilities,          // Tier 1 — always populated
    pub topology: Option<HardwareTopology>,  // Tier 2 — None during first probe
}
```

### System Identity Duplication (Intentional)

`system_manufacturer` and `system_product` remain in `HardwareInventory`
(Tier 1) and are duplicated in `SystemIdentity` (Tier 2). Tier 1 provides the
quick values used for offering manifest matching; Tier 2 enriches with serial
number, BIOS version, and BIOS date. The DMI read is fast enough for the Tier 1
path. Removing it from Tier 1 would break offering matching during the topology
probe window.

### Detection Sources — Platform Parity

A core requirement is 1:1 detection parity between Linux and Windows. Every
field in `HardwareTopology` must be obtainable on both platforms. The following
table documents the specific API for each subsystem on each platform, the Rust
access path, and the parity status.

#### Unified Windows API: `cfgmgr32`

The `cfgmgr32.dll` Configuration Manager API family
(`windows::Win32::Devices::DeviceAndDriverInstallation`) is the primary
detection surface on Windows. It covers PCI, USB, Thunderbolt, and general
device enumeration through one consistent interface — no process spawning, no
WMI COM overhead. The key function is `CM_Get_DevNode_PropertyW` with
`DEVPKEY_*` property keys, which is the native API behind PowerShell's
`Get-PnpDeviceProperty`.

WMI (`Win32_*` classes via the `wmi` Rust crate) is used only as a fallback
for subsystems where `cfgmgr32` does not expose the required data (slot
inventory, chassis type).

#### PCIe Topology (link speed, lane width, generation)

| Detail | Linux | Windows |
|--------|-------|---------|
| Device enumeration | `lspci -n` or sysfs `/sys/bus/pci/devices/*/` | `CM_Get_Device_ID_ListW("PCI", ...)` — single call, all PCI IDs |
| BDF address | sysfs directory name (`0000:01:00.0`) | `CM_Get_DevNode_PropertyW` + `DEVPKEY_Device_LocationInfo` |
| Max link width | `/sys/bus/pci/devices/*/max_link_width` | `DEVPKEY_PciDevice_MaxLinkWidth` (Win10+) |
| Negotiated width | `/sys/bus/pci/devices/*/current_link_width` | `DEVPKEY_PciDevice_CurrentLinkWidth` |
| Max link speed | `/sys/bus/pci/devices/*/max_link_speed` | `DEVPKEY_PciDevice_MaxLinkSpeed` |
| Negotiated speed | `/sys/bus/pci/devices/*/current_link_speed` | `DEVPKEY_PciDevice_CurrentLinkSpeed` |
| Device class | `/sys/bus/pci/devices/*/class` | `DEVPKEY_PciDevice_BaseClass` + `SubClass` |
| Device name | `lspci -v` (decoded) | `DEVPKEY_Device_FriendlyName` |
| Driver | `/sys/bus/pci/devices/*/driver` → symlink name | `DEVPKEY_Device_DriverDesc` |
| Rust crate | `std::fs::read_to_string` on sysfs | `windows` crate `cfgmgr32` bindings |

**Parity: Full.** The `DEVPKEY_PciDevice_*` keys (Windows 10+) expose the same
PCIe capability register data that Linux exposes via sysfs. No kernel driver or
process spawn required.

#### M.2 Slot Inventory

| Detail | Linux | Windows |
|--------|-------|---------|
| Slot list (empty + occupied) | `dmidecode -t 9` (SMBIOS Type 9) | `Win32_SystemSlot` (WMI, reads same SMBIOS Type 9) |
| Slot designation/label | Type 9 `Slot Designation` field | `Win32_SystemSlot.SlotDesignation` |
| Current usage | Type 9 `Current Usage` (In Use / Available) | `Win32_SystemSlot.CurrentUsage` |
| Bus number mapping | Type 9 `Bus Address` field | `Win32_SystemSlot.BusNumber` → correlate with PCI device |
| NVMe occupants | `lspci` class `0108` + bus correlation | `MSFT_PhysicalDisk` where `BusType = 17` (NVMe) |
| WiFi occupants | `lspci` class `0280` + bus correlation | PnP device class `Net` on M.2 bus |
| Rust crate | `Command::new("dmidecode")` or parse SMBIOS raw | `wmi` crate for `Win32_SystemSlot` |

**Parity: Full (BIOS-dependent).** Both platforms read SMBIOS Type 9 tables.
Quality depends on the BIOS populating the table — budget boards sometimes omit
M.2 entries. This limitation is symmetric on both platforms.

#### Thunderbolt / USB4 / OCuLink

| Detail | Linux | Windows |
|--------|-------|---------|
| Controller presence | `boltctl list` or sysfs `/sys/bus/thunderbolt/` | PnP class `Thunderbolt` or `USB4` (Win11 24H2+) |
| Controller model | sysfs device attributes | `DEVPKEY_Device_FriendlyName` on TB device node |
| Version detection | sysfs `generation` attribute | PCI device ID lookup table (controller chip → TB version) |
| Firmware version | sysfs or `boltctl` | `DEVPKEY_Device_FirmwareVersion` |
| Rust crate | sysfs read or `Command::new("boltctl")` | `cfgmgr32` + PCI ID → version mapping |

**Thunderbolt version mapping** (PCI device IDs):

```
Intel Light Ridge   (8086:1513, 151A, 151B)       → Thunderbolt 1
Intel Falcon Ridge  (8086:156C, 156D)              → Thunderbolt 2
Intel Alpine Ridge  (8086:15D2, 15D9, 15DA)        → Thunderbolt 3
Intel Titan Ridge   (8086:15E7, 15EA, 15EB)        → Thunderbolt 3
Intel Maple Ridge   (8086:9A1B, 9A1D, 9A1F)       → Thunderbolt 4 / USB4
Intel Barlow Ridge  (8086:A73E, A73F)              → USB4 v2
```

This lookup table is embedded in the detection code. OCuLink is electrically
PCIe — it appears as a standard PCI device and is detected through the PCIe
topology probe, not a dedicated subsystem.

**Parity: Full.** Linux exposes a richer topology tree (`boltctl`), but the
data points we need (presence, version, bandwidth, firmware) are available on
both platforms. Version detection via PCI ID table is platform-agnostic.

#### USB Enumeration

| Detail | Linux | Windows |
|--------|-------|---------|
| Controller list | sysfs `/sys/bus/usb/devices/usb*/` | `Win32_USBController` or `cfgmgr32` class `USB` |
| Controller version | sysfs `version` attribute | Controller name contains "xHCI" / "EHCI" / "OHCI" |
| Device list | sysfs `/sys/bus/usb/devices/*/` | `CM_Get_Device_ID_ListW("USB", ...)` |
| Device speed | sysfs `speed` attribute (Mbps) | `DEVPKEY_Device_Speed` (enum: 1=Low → 6=SS+20G) |
| VID:PID | sysfs `idVendor` / `idProduct` | Device instance ID contains `VID_XXXX&PID_XXXX` |
| Product name | sysfs `product` | `DEVPKEY_Device_FriendlyName` |
| Rust crate | sysfs read | `cfgmgr32` |

**Windows `DEVPKEY_Device_Speed` values:**

| Value | USB Version | Speed |
|-------|------------|-------|
| 1 | Low Speed | 1.5 Mbps |
| 2 | Full Speed | 12 Mbps |
| 3 | USB 2.0 | 480 Mbps |
| 4 | USB 3.0 | 5 Gbps |
| 5 | USB 3.1 Gen 2 | 10 Gbps |
| 6 | USB 3.2 Gen 2×2 | 20 Gbps |

**Parity: Full.**

#### Network Interfaces

| Detail | Linux | Windows |
|--------|-------|---------|
| Interface list | sysfs `/sys/class/net/*/` | `GetAdaptersAddresses` (`iphlpapi`) or `MSFT_NetAdapter` |
| Link speed | sysfs `speed` (Mbps) or `ethtool` | `MSFT_NetAdapter.LinkSpeed` or `GetAdaptersAddresses.TransmitLinkSpeed` |
| Interface type | sysfs `type` + driver heuristics | `MSFT_NetAdapter.MediaType` |
| MAC address | sysfs `address` | `GetAdaptersAddresses.PhysicalAddress` |
| NIC firmware | `ethtool -i` (`firmware-version`) | `DEVPKEY_Device_FirmwareVersion` or Intel `*NVMVersion` registry |
| PCIe bus info | `ethtool -i` (`bus-info`) | `Get-NetAdapterHardwareInfo` → `PcieLinkSpeed`, `PcieLinkWidth` |
| Rust crate | sysfs read or `Command::new("ethtool")` | `windows` crate `iphlpapi` + `cfgmgr32` for firmware |

**Parity: Full.** NIC firmware version is driver-dependent on both platforms
(Intel reliably exposes it; Realtek often does not). The limitation is
symmetric.

#### Firmware Inventory

| Detail | Linux | Windows |
|--------|-------|---------|
| BIOS version + date | `dmidecode -t bios` | `Win32_BIOS.SMBIOSBIOSVersion` + `ReleaseDate` |
| SSD/HDD firmware | `smartctl` or `hdparm -I` | `MSFT_PhysicalDisk.FirmwareVersion` |
| GPU driver version | `/sys/module/nvidia/version` or `modinfo` | `Win32_VideoController.DriverVersion` |
| Component-level scan | `fwupdmgr get-devices --json` | Multi-source (see below) |
| UEFI firmware table | N/A (fwupd abstracts) | ESRT registry `HKLM\HARDWARE\UEFI\ESRT\{guid}\` |
| Broad device sweep | fwupd covers all | `DEVPKEY_Device_FirmwareVersion` across all PnP nodes |
| Motherboard info | `dmidecode -t baseboard` | `Win32_BaseBoard.Manufacturer` + `Product` + `Version` |
| Rust crate | `Command::new("fwupdmgr")` + `Command::new("dmidecode")` | `wmi` crate + `RegOpenKeyExW` for ESRT |

**Windows firmware detection strategy** (ordered by reliability):

1. `Win32_BIOS` — BIOS vendor, version, date (always available)
2. `MSFT_PhysicalDisk` — SSD/HDD firmware version (always available)
3. `Win32_VideoController` — GPU driver version (always available)
4. ESRT registry — UEFI firmware resources with version + update status
5. `DEVPKEY_Device_FirmwareVersion` sweep — broad scan across all PnP devices
6. `Win32_BaseBoard` — motherboard identity and revision

Linux has `fwupdmgr` as a single unified firmware inventory. Windows requires
combining multiple sources. The detection module abstracts this: both platforms
emit the same `Vec<FirmwareComponent>`, regardless of how many queries it took.

**Parity: Functional.** Both platforms produce equivalent firmware inventories.
Windows requires more queries but covers the same ground. The `updatable` field
maps to fwupd on Linux and ESRT `LastAttemptStatus` on Windows.

#### System Identity

| Detail | Linux | Windows |
|--------|-------|---------|
| Manufacturer | `dmidecode -t system` → `Manufacturer` | `Win32_ComputerSystemProduct.Vendor` |
| Product name | `dmidecode -t system` → `Product Name` | `Win32_ComputerSystemProduct.Name` |
| Serial number | `dmidecode -t system` → `Serial Number` | `Win32_ComputerSystemProduct.IdentifyingNumber` |
| UUID | `dmidecode -t system` → `UUID` | `Win32_ComputerSystemProduct.UUID` |
| Chassis type | `dmidecode -t chassis` → `Type` | `Win32_SystemEnclosure.ChassisTypes[]` |
| Chassis serial | `dmidecode -t chassis` → `Serial Number` | `Win32_SystemEnclosure.SerialNumber` |
| Raw SMBIOS | `/sys/firmware/dmi/tables/` | `GetSystemFirmwareTable(RSMB, 0, ...)` |
| Rust crate | `Command::new("dmidecode")` or raw table parse | `wmi` crate or `GetSystemFirmwareTable` (fastest) |

`GetSystemFirmwareTable` with `RSMB` signature returns the raw SMBIOS binary —
the same data that `dmidecode` parses on Linux. A single Win32 syscall, no WMI
overhead. The Rust parser is shared between platforms: both feed raw SMBIOS
bytes into the same struct deserializer.

**Parity: Full.**

#### PCIe Fingerprint (Delta Detection)

| Detail | Linux | Windows |
|--------|-------|---------|
| Method | `lspci -n` output (VEN:DEV per device) | `CM_Get_Device_ID_ListW("PCI", ...)` |
| Latency | ~2ms | ~5–10ms |
| Process spawn | Optional (`lspci`) or sysfs read (no spawn) | No spawn (native Win32 call) |
| Output | `BBBB:DD.F CCCC: VVVV:DDDD` per line | Multi-string of `PCI\VEN_XXXX&DEV_XXXX&...` |
| Hash input | Sorted VEN:DEV pairs | Sorted VEN:DEV pairs (extracted from instance IDs) |
| Rust crate | sysfs directory enumeration | `cfgmgr32` |

Both platforms produce a sorted list of PCI vendor:device pairs. The SHA-256
hash is computed over the same canonical format regardless of platform, so the
fingerprint is comparable across OS reboots (e.g., dual-boot stone).

**Parity: Full.**

#### Summary

| Subsystem | Linux API | Windows API | Spawn-Free | Parity |
|-----------|-----------|-------------|------------|--------|
| PCIe topology | sysfs | `cfgmgr32` `DEVPKEY_PciDevice_*` | Both | Full |
| M.2 slots | `dmidecode -t 9` | `Win32_SystemSlot` (WMI) | Linux: no, Windows: no | Full |
| Thunderbolt | sysfs + `boltctl` | `cfgmgr32` + PCI ID table | Both | Full |
| USB | sysfs | `cfgmgr32` `DEVPKEY_Device_Speed` | Both | Full |
| Network | sysfs + `ethtool` | `iphlpapi` + `cfgmgr32` | Both | Full |
| Firmware | `fwupdmgr` + `dmidecode` | WMI + ESRT registry + PnP sweep | Linux: no, Windows: mixed | Functional |
| System identity | `dmidecode -t system` | `GetSystemFirmwareTable(RSMB)` | Both | Full |
| Fingerprint | sysfs enum | `CM_Get_Device_ID_ListW` | Both | Full |

Every field in the `HardwareTopology` struct is obtainable on both platforms.
Where possible, detection avoids process spawning by using sysfs (Linux) and
`cfgmgr32` / `GetSystemFirmwareTable` (Windows) directly. WMI is reserved for
subsystems that require SMBIOS table interpretation (slot inventory, chassis
type) — these are inherently slow but run in the background Tier 2 probe where
latency is acceptable.

### Rust Implementation Strategy

#### Crate Selection

| Subsystem | Crate | Version | Platform | Role |
|-----------|-------|---------|----------|------|
| SMBIOS/DMI | **smbios-lib** | 0.9.x | Linux + Windows + macOS | System identity, slot inventory (Type 9), chassis type. 1.1M downloads, full SMBIOS 3.7.0 coverage. Reads raw tables via `GetSystemFirmwareTable(RSMB)` on Windows, `/sys/firmware/dmi/tables/` on Linux. |
| WMI queries | **wmi** | 0.18.x | Windows only | Fallback for `Win32_SystemSlot`, `Win32_BIOS`, `Win32_NetworkAdapter`. Serde-based deserialization, async queries. 2.7M downloads. |
| USB enumeration | **nusb** | 0.2.x | Linux + Windows + macOS | Pure Rust, no C dependency. Exposes `Speed` enum (Low/Full/High/Super/SuperPlus). Async-native. 560K downloads. |
| NIC metadata | **netdev** | 0.41.x | Linux + Windows + macOS + Android | Interface name, MAC, type, MTU, operational state. 1.4M downloads. Does **not** expose link speed — supplement with platform-specific calls. |
| PCI ID lookup | **pci-ids** | 0.2.x | All (data-only) | Vendor:device name resolution from the PCI ID Repository. 550K downloads. |
| Windows API | **windows** | 0.62.x | Windows only | `cfgmgr32` bindings (`CM_Get_DevNode_PropertyW`, `CM_Get_Device_ID_ListW`), `GetSystemFirmwareTable`, `GetAdaptersAddresses`. Microsoft-maintained, 205M downloads. |

#### Platform-Specific Code (No Crate Available)

**PCIe link speed and lane width** — no cross-platform crate exposes this.
Custom platform code is required:

- **Linux**: read sysfs attributes directly:
  ```
  /sys/bus/pci/devices/{BDF}/current_link_speed   → "8.0 GT/s PCIe"
  /sys/bus/pci/devices/{BDF}/current_link_width    → "8"
  /sys/bus/pci/devices/{BDF}/max_link_speed        → "8.0 GT/s PCIe"
  /sys/bus/pci/devices/{BDF}/max_link_width        → "16"
  ```
- **Windows**: `cfgmgr32` with hand-defined `DEVPKEY_PciDevice_*` constants.
  These DEVPKEYs are documented in the Windows WDK (`devpkey.h`) but are not
  included in the `windows` crate's `Properties` module. Define them as
  `DEVPROPKEY` structs using the known GUIDs and property IDs:
  ```rust
  // DEVPKEY_PciDevice_CurrentLinkSpeed
  // {3AB22E31-8264-4B4E-9AF5-A8D2D8E33E62}, 9
  const DEVPKEY_PCI_CURRENT_LINK_SPEED: DEVPROPKEY = DEVPROPKEY {
      fmtid: GUID::from_u128(0x3AB22E31_8264_4B4E_9AF5_A8D2D8E33E62),
      pid: 9,
  };
  ```

**NIC link speed** — `netdev` does not expose this. Supplement with:

- **Linux**: sysfs `/sys/class/net/{iface}/speed` (Mbps as integer) or
  `ethtool` ioctl (`ETHTOOL_GLINKSETTINGS`).
- **Windows**: `GetIfEntry2` from `iphlpapi` (`TransmitLinkSpeed` /
  `ReceiveLinkSpeed` in bits/sec) or WMI `MSFT_NetAdapter.LinkSpeed`.

**NIC firmware version** — driver-dependent on both platforms:

- **Linux**: `ethtool -i {iface}` → `firmware-version` field.
- **Windows**: `DEVPKEY_Device_FirmwareVersion` on the NIC's PnP device node.
  Intel NICs also expose NVM version via registry key `*NVMVersion`.

Both platforms may return `None` for NICs that don't expose firmware versions
(common with Realtek). This limitation is symmetric and documented in the
`NetworkInterface.firmware_version: Option<String>` field.

#### Module Layout

```
src/common/src/types/topology.rs        — Tier 2 type definitions
src/moss/src/infra/topology/mod.rs      — detection orchestration + shared logic
src/moss/src/infra/topology/pcie.rs     — PCIe enumeration (platform-specific)
src/moss/src/infra/topology/smbios.rs   — SMBIOS parsing (slots, identity, chassis)
src/moss/src/infra/topology/usb.rs      — USB enumeration via nusb
src/moss/src/infra/topology/network.rs  — NIC detection (netdev + platform speed)
src/moss/src/infra/topology/firmware.rs — firmware inventory (fwupd / WMI + ESRT)
src/moss/src/infra/topology/fingerprint.rs — quick PCI fingerprint for delta detection
src/moss/src/tasks/topology_probe.rs    — background probe task
src/moss/src/api/v1/capabilities.rs     — extended endpoints
src/common/src/client/stone_api.rs      — extended StoneApi capabilities family
```

Each subsystem module exposes a single `pub async fn detect_*() -> Result<T>`
function. The orchestrator in `mod.rs` calls them in sequence (PCIe → SMBIOS →
USB → network → firmware), updating `TopologyStatus` from `Probing` → `Partial`
→ `Complete` as subsystems finish. Each subsystem is independent — a failure in
USB detection does not block firmware detection.

## Rationale

- **Tier separation preserves startup speed.** Offering compatibility checks
  depend on Tier 1 data available within seconds. Tier 2 probes can take 5–10
  seconds on Windows (WMI, PowerShell) and should never block that path.
- **Delta-gated reprobing avoids waste.** Most boots see identical hardware.
  A 2ms fingerprint check avoids a 5–10s full probe on every startup.
- **Caching with fingerprint is self-correcting.** Hot-plugging an eGPU changes
  the PCI device list, which changes the fingerprint, which triggers re-probe
  automatically.
- **Periodic re-probe as safety net.** Firmware updates and BIOS changes don't
  alter the PCI device list but can change negotiated link widths, power budgets,
  and feature flags. A 24h sweep catches these.
- **Garden aggregation enables fleet management.** Grouping by product and
  surfacing discrepancies turns individual stone data into actionable fleet
  intelligence.
- **Firmware capture supports debugging.** Two identical HP t630s behaving
  differently is often explained by BIOS version differences. Capturing firmware
  versions makes this visible without SSH-ing into each stone.

## Consequences

### Positive

- Fleet-wide hardware visibility from a single command (`rake garden inspect`).
- eGPU expansion planning becomes data-driven — which stones have empty PCIe
  slots, at what bandwidth, with what power budget.
- Discrepancy detection across nominally identical hardware (RAM, firmware,
  expansion configuration).
- Hot-plug detection for eGPUs: plug in a card, fingerprint changes, re-probe
  runs, topology updates, garden sees the new GPU.
- Backwards compatible: `/capabilities/core` returns the same shape as the
  previous `/capabilities` endpoint. Existing consumers unaffected.
- Agentic onboarding scales manifest coverage to the fleet. Every new stone
  model that joins the garden gets a draft manifest scaffolded from detected
  data, rather than requiring a human to write one from scratch.
- The manifest schema grows to encode expansion topology (PCIe slots, M.2 bays,
  eGPU viability), making fleet-bible knowledge machine-readable and queryable
  by the compatibility engine.

### Negative

- Platform-specific detection code increases maintenance surface (lspci vs WMI,
  sysfs vs registry). Each platform path must be tested independently.
- M.2 empty-slot detection on Linux requires DMI table parsing (`dmidecode`),
  which needs root. If Moss runs unprivileged, empty slots may not be visible —
  only occupied slots appear via sysfs.
- Windows WMI queries can be slow (~5–10s) and occasionally hang. Timeouts are
  mandatory on every WMI call.
- Topology cache file adds another persistence artifact to manage during
  upgrades and migrations.
- Agentic research (Stage 2) depends on external sources (LVFS, spec sheets,
  community forums). Availability and accuracy of these sources is not
  guaranteed. Generated manifests must always be treated as drafts.
- The `HwProfile` schema expansion (expansion slots, RAM type, eGPU viability)
  requires updating the manifest loader and any code that reads hardware
  profiles. Existing manifests (wyse-5070) need the new fields added.

### Neutral

- `system_manufacturer` and `system_product` are intentionally duplicated across
  Tier 1 and Tier 2. This is a pragmatic choice, not technical debt.
- Serial number is captured when available. No opt-out mechanism; the data stays
  local to the stone and garden — it is never transmitted externally.
- The 24-hour periodic re-probe interval is a starting default. It may be tuned
  based on operational experience.
- Draft manifests are stored locally on the stone that triggered onboarding.
  They do not propagate to other stones or become embedded in the binary until
  a human promotes them.

## Agentic Hardware Onboarding

### Problem

Today there is one hand-crafted hardware manifest (`hw/dell/wyse-5070`). Each
manifest requires deep knowledge: DMI identity patterns, firmware LVFS IDs,
TDP/idle wattage, BIOS access keys, form factor classification, and
bidirectional compatibility rules with scoring adjustments. Writing these by
hand does not scale to a heterogeneous fleet where every new stone model
(HP t630, Lenovo M720q, Intel NUC 12, Beelink EQR6) needs a manifest before
compatibility checks and placement scoring work correctly.

### What Tier 2 Provides Automatically

The topology probe captures most of what the `identity` and `profile` sections
of a hardware manifest need:

| Manifest Field | Tier 2 Source |
|----------------|---------------|
| `identity.system_manufacturer` | `SystemIdentity.manufacturer` |
| `identity.system_product_name_patterns` | `SystemIdentity.product` |
| `firmware.versions.current` | `FirmwareComponent` (BIOS entry) |
| `profile.cpu_architecture` | Tier 1 `CpuCapabilities.architecture` |
| `profile.cpu_cores` | Tier 1 `CpuCapabilities.cores` |
| `profile.storage_type` | Tier 1 `DiskCapabilities.disk_type` |
| `profile.storage_expandable` | `Expansion.m2` (empty M.2 slots) |
| **NEW** `profile.expansion` | `Expansion.pcie`, `Expansion.m2`, `Expansion.thunderbolt` |

### What Requires External Knowledge

| Manifest Field | Why It Can't Be Detected |
|----------------|--------------------------|
| `profile.tdp_watts` / `idle_watts` / `max_watts` | Not exposed by hardware; requires spec sheet |
| `profile.fanless` | No reliable sensor; requires product knowledge |
| `bios.access_key` / `boot_menu_key` | BIOS vendor-specific, not queryable at runtime |
| `firmware.lvfs_device_id` | LVFS catalog lookup required |
| `firmware.versions.recommended` / `latest_known` | Requires LVFS or vendor changelog |
| Compatibility `recommended` / `caution` / `not_recommended` | Requires reasoning about hardware limits vs service demands |
| Scoring `boost` / `penalty` adjustments | Requires understanding workload–hardware fit |

### Agentic Onboarding Pipeline

When a stone reports hardware that matches no existing manifest, the system
triggers an agentic onboarding process. This runs as a background job — never
blocking the stone's operation — and produces a draft manifest for human review.

**Trigger**: Tier 1 detection completes, `HwManifests::find_matching()` returns
`None` for the detected `system_manufacturer + system_product`.

**Pipeline stages**:

```
Stage 1: Scaffold (automatic, seconds)
├── Extract identity, profile, expansion from Tier 1 + Tier 2 data
├── Generate manifest.yaml with all detectable fields populated
├── Generate frontmatter.json with release_year, form_factor, tags
└── Mark manifest as status: draft

Stage 2: Research (agentic, minutes)
├── Query LVFS for firmware device IDs and latest versions
├── Search manufacturer spec sheets for TDP, idle watts, max watts
├── Search community sources (ServeTheHome, egpu.io, parkytowers)
│   for real-world expansion builds and compatibility reports
├── Cross-reference fleet-bible if the model is documented there
├── Determine BIOS access keys from vendor documentation
└── Populate fields that Stage 1 could not

Stage 3: Compatibility Reasoning (agentic, minutes)
├── Analyse hardware profile against known offering requirements:
│   ├── CPU flags → AVX/AVX2 dependent offerings (ollama, milvus, weaviate)
│   ├── Storage type → write-endurance warnings (eMMC, SD)
│   ├── RAM ceiling → memory-hungry offering penalties
│   ├── PCIe expansion → eGPU potential, future capability projection
│   └── TDP → power-constrained environment adjustments
├── Generate compatibility.yaml with:
│   ├── recommended / caution / not_recommended lists
│   ├── Warning rules with reasons and suggestions
│   └── Scoring adjustments (boost for lightweight, penalty for write-heavy)
└── Cross-validate against existing manifests for similar hardware
    (e.g., if HP t630 ≈ HP t640, inherit and adjust)

Stage 4: Review (human, async)
├── Present draft manifest via rake command or dashboard
├── Human approves, adjusts, or rejects
├── Approved manifests are committed to the embedded manifest directory
└── Rejected drafts are discarded with reason (feeds future improvements)
```

**Key design principles**:

- **Draft-first, never auto-authoritative.** Generated manifests are `status: draft`
  until a human promotes them to `status: accepted`. No auto-generated manifest
  ever influences placement scoring or compatibility warnings in production
  without review.
- **Incremental enrichment.** Stage 1 runs immediately and produces a useful
  scaffold. Stage 2 and 3 may take minutes and may fail partially (LVFS has no
  entry, spec sheet not found). Each stage enriches what the previous produced.
  A manifest with only Stage 1 data is still useful — it has identity, expansion
  topology, and detected capabilities.
- **Fleet learning.** When multiple stones of the same model join, the system
  compares their Tier 2 data. Discrepancies (different RAM, different BIOS
  version, different M.2 occupants) are surfaced as notes in the draft manifest,
  giving the human reviewer a fuller picture of the model's configuration space.
- **Inheritance from similar hardware.** If `hp/t640` has no manifest but
  `hp/t630` does, Stage 3 can propose a compatibility.yaml that inherits from
  the t630's and adjusts for known differences (different CPU generation,
  different RAM ceiling). The human reviewer sees the delta, not a blank slate.

### Hardware Manifest Schema Extension

The `HwProfile` struct grows to capture expansion topology from Tier 2 data:

```rust
pub struct HwProfile {
    // Existing fields
    pub cpu_architecture: Option<String>,
    pub cpu_cores: Option<u32>,
    pub storage_type: Option<String>,
    pub storage_expandable: Option<bool>,
    pub fanless: Option<bool>,
    pub tdp_watts: Option<u32>,
    pub idle_watts: Option<u32>,
    pub max_watts: Option<u32>,
    pub form_factor: Option<String>,

    // NEW — expansion topology (populated from Tier 2 detection)
    pub expansion: Option<HwExpansionProfile>,
}

pub struct HwExpansionProfile {
    /// PCIe slots: physical width, generation, power delivery
    pub pcie_slots: Vec<HwPcieSlot>,
    /// M.2 slots: key type, supported form factors
    pub m2_slots: Vec<HwM2Slot>,
    /// Thunderbolt/OCuLink/USB4 ports
    pub high_bandwidth_ports: Vec<HwHighBandwidthPort>,
    /// Maximum RAM capacity (from spec sheet, not detected)
    pub max_ram_gb: Option<u32>,
    /// RAM slot count
    pub ram_slots: Option<u8>,
    /// RAM generation (DDR4, DDR5)
    pub ram_type: Option<String>,
    /// eGPU viability assessment (computed from slots + bandwidth + power)
    pub egpu_viability: Option<EgpuViability>,
}

pub enum EgpuViability {
    /// Native PCIe x16/x8: full-speed GPU, no adapter needed
    Native { bandwidth_gbps: f32 },
    /// Via adapter (M.2, OCuLink, Thunderbolt): viable with penalty
    Adapter { method: String, bandwidth_gbps: f32, penalty_estimate: String },
    /// Theoretically possible but impractical (PCIe x1, low power)
    Marginal { method: String, reason: String },
    /// No known expansion path
    None,
}

pub struct HwPcieSlot {
    pub physical_width: u8,    // x1, x4, x8, x16
    pub generation: u8,        // 3, 4, 5
    pub slot_power_w: Option<u8>,
    pub form_factor: String,   // "full-height", "low-profile", "riser"
}

pub struct HwM2Slot {
    pub key: String,           // "M", "E", "A+E", "B+M"
    pub form_factors: Vec<String>, // ["2230", "2242", "2280"]
    pub pcie_lanes: Option<u8>,
    pub sata: bool,            // supports SATA protocol
}

pub struct HwHighBandwidthPort {
    pub kind: String,          // "thunderbolt", "oculink", "usb4"
    pub version: Option<String>,
    pub bandwidth_gbps: f32,
}
```

This makes the manifest a complete hardware reference — what the fleet-bible
documents per model today becomes structured, queryable data that the
compatibility engine and placement scorer can reason about.

### Example: Auto-Generated HP t630 Manifest (Draft)

Given Tier 2 topology data from a live HP t630 stone:

```yaml
# AUTO-GENERATED — status: draft — requires human review
name: t630
vendor: hp
type: hardware

identity:
  system_manufacturer: "HP"
  system_product_name_patterns:
    - "t630 Thin Client"
    - "HP t630"

firmware:
  method: fwupd                      # detected: fwupd available
  versions:
    current: "P92 v02.37"           # detected: BIOS version from DMI
    # minimum: ???                   # NEEDS HUMAN INPUT
    # recommended: ???               # NEEDS HUMAN INPUT
    # latest_known: ???              # NEEDS RESEARCH (LVFS / HP support)
  requires_reboot: true

profile:
  cpu_architecture: "x86_64"         # detected
  cpu_cores: 4                       # detected
  storage_type: "emmc"               # detected (or "ssd" if M.2 SATA added)
  storage_expandable: true           # detected: empty M.2 Key M slot
  fanless: false                     # NEEDS VERIFICATION (t630 has a fan)
  # tdp_watts: ???                   # NEEDS RESEARCH
  # idle_watts: ???                  # NEEDS RESEARCH
  # max_watts: ???                   # NEEDS RESEARCH
  form_factor: "thin-client"         # inferred from product name

  expansion:
    pcie_slots: []                   # detected: none
    m2_slots:
      - key: "E"                     # detected: M.2 Key E occupied by WiFi
        form_factors: ["2230"]
        pcie_lanes: 1
        sata: false
    high_bandwidth_ports: []         # detected: none
    max_ram_gb: 64                   # NEEDS VERIFICATION (community reports)
    ram_slots: 2                     # detected via DMI
    ram_type: "DDR4"                 # detected
    egpu_viability:
      method: "M.2 Key E adapter + PCIe x1 riser"
      bandwidth_gbps: 5.0
      penalty_estimate: "High for loading, negligible for inference"

bios:
  # access_key: ???                  # NEEDS RESEARCH
  # boot_menu_key: ???               # NEEDS RESEARCH
  boot_mode: "UEFI"                 # detected
```

Fields marked with `# NEEDS` are gaps that Stage 2 (research) and Stage 4
(human review) fill. The scaffold is immediately useful for identity matching
and basic profile data.

## References

- [ARCH-0012](ARCH-0012-typed-stone-api-client.md) — StoneApi typed client (extended by this ADR)
- [ARCH-0013](ARCH-0013-capability-gap-closure.md) — capability audit that identified detection gaps
- Fleet Bible — `outputs/fleet-bible/02_pcie_expansion_guide.md` (PCIe slot reference)
- Fleet Bible — `outputs/fleet-bible/03_egpu_methods_and_builds.md` (eGPU connection methods)
- Implementation: `src/common/src/types/hardware.rs` (current Tier 1 types)
- Implementation: `src/moss/src/infra/hardware.rs` (current detection logic)
- Implementation: `src/moss/src/tasks/hardware_detection.rs` (current background task)
- Implementation: `src/common/src/manifests/hw.rs` (hardware manifest loader)
- Manifest: `src/moss/embedded/manifests/hw/dell/wyse-5070.manifest.yaml` (reference manifest)
