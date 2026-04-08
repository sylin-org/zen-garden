//! Hardware topology detection — ARCH-0014 Tier 2.
//!
//! Each submodule exposes a single `pub async fn detect_*() -> Result<T>` function.
//! The orchestrator calls them in sequence, updating `TopologyStatus` as
//! subsystems complete. Each subsystem is independent — a failure in one
//! does not block the others.

pub mod fingerprint;
pub mod smbios;
pub mod pcie;
pub mod firmware;
pub mod network;
pub mod usb;

use anyhow::Result;
use garden_common::types::hardware_topology::{HardwareTopology, TopologyStatus};

/// Run the full topology probe. Returns a complete `HardwareTopology`.
///
/// Called from the background task after fingerprint comparison detects a change
/// (or on explicit refresh). Each subsystem logs its own errors and returns
/// partial data rather than failing the entire probe.
pub async fn probe_full_topology(fingerprint: String) -> Result<HardwareTopology> {
    tracing::info!("probe_full_topology: starting subsystem probes");
    let probe_start = chrono::Utc::now();

    // probe_version is stamped by the caller (topology_probe.rs) after probe completes
    let mut topo = HardwareTopology::probing(fingerprint, 0);

    // SMBIOS — system identity, M.2 slots, chassis type
    tracing::info!("probe_full_topology: [1/5] SMBIOS...");
    match smbios::detect_smbios().await {
        Ok(result) => {
            topo.system = result.identity;
            topo.expansion.m2 = result.m2_slots;
            topo.memory = result.memory;
            topo.status = TopologyStatus::Partial;
            tracing::info!(
                manufacturer = %topo.system.manufacturer,
                product = %topo.system.product,
                m2_slots = topo.expansion.m2.len(),
                memory_slots = topo.memory.slots.len(),
                "probe_full_topology: [1/5] SMBIOS complete"
            );
        }
        Err(e) => tracing::warn!(error = %e, "probe_full_topology: [1/5] SMBIOS FAILED — continuing"),
    }

    // PCIe devices — link speed, width, generation
    tracing::info!("probe_full_topology: [2/5] PCIe...");
    match pcie::detect_pcie_devices().await {
        Ok(devices) => {
            let tb_ports = pcie::extract_thunderbolt_ports(&devices);
            topo.expansion.pcie = devices;
            topo.expansion.thunderbolt = tb_ports;
            tracing::info!(
                pcie_devices = topo.expansion.pcie.len(),
                thunderbolt_ports = topo.expansion.thunderbolt.len(),
                "probe_full_topology: [2/5] PCIe complete"
            );
        }
        Err(e) => tracing::warn!(error = %e, "probe_full_topology: [2/5] PCIe FAILED — continuing"),
    }

    // Firmware inventory
    tracing::info!("probe_full_topology: [3/5] Firmware...");
    match firmware::detect_firmware().await {
        Ok(components) => {
            topo.firmware = components;
            tracing::info!(
                components = topo.firmware.len(),
                "probe_full_topology: [3/5] Firmware complete"
            );
        }
        Err(e) => tracing::warn!(error = %e, "probe_full_topology: [3/5] Firmware FAILED — continuing"),
    }

    // Network interfaces
    tracing::info!("probe_full_topology: [4/5] Network...");
    match network::detect_network_interfaces().await {
        Ok(interfaces) => {
            topo.network = interfaces;
            tracing::info!(
                interfaces = topo.network.len(),
                "probe_full_topology: [4/5] Network complete"
            );
        }
        Err(e) => tracing::warn!(error = %e, "probe_full_topology: [4/5] Network FAILED — continuing"),
    }

    // USB summary
    tracing::info!("probe_full_topology: [5/5] USB...");
    match usb::detect_usb().await {
        Ok(summary) => {
            topo.expansion.usb = summary;
            tracing::info!(
                port_groups = topo.expansion.usb.ports.len(),
                connected = topo.expansion.usb.connected_devices.len(),
                "probe_full_topology: [5/5] USB complete"
            );
        }
        Err(e) => tracing::warn!(error = %e, "probe_full_topology: [5/5] USB FAILED — continuing"),
    }

    topo.status = TopologyStatus::Complete;
    topo.probed_at = probe_start.to_rfc3339();

    tracing::info!("probe_full_topology: all subsystems done");
    Ok(topo)
}
