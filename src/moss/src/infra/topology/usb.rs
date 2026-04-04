//! USB port enumeration (ARCH-0014).
//!
//! - Linux: sysfs `/sys/bus/usb/devices/*/`
//! - Windows: `nusb` crate (pure Rust, cross-platform USB enumeration)
//!
//! nusb works on both platforms, but sysfs gives richer controller-level
//! data on Linux. We use nusb as the primary path for device enumeration
//! and supplement with sysfs on Linux for port group counting.

use anyhow::Result;
use garden_common::types::hardware_topology::{UsbDevice, UsbPortGroup, UsbSummary};
use nusb::MaybeFuture;

/// Detect USB port groups and connected devices.
pub async fn detect_usb() -> Result<UsbSummary> {
    tokio::task::spawn_blocking(detect_usb_blocking).await?
}

fn detect_usb_blocking() -> Result<UsbSummary> {
    let mut port_counts: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    let mut connected = Vec::new();

    // nusb enumerates connected devices cross-platform
    if let Ok(devices) = nusb::list_devices().wait() {
        for info in devices {
            let speed = info.speed();
            let version = usb_speed_to_version(speed);

            *port_counts.entry(version.clone()).or_insert(0) += 1;

            let vendor = format!("{:04x}", info.vendor_id());
            let product_name = info
                .product_string()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{:04x}", info.product_id()));

            connected.push(UsbDevice {
                vendor,
                product: product_name,
                bus_version: version,
            });
        }
    }

    // Convert counts to port groups
    let mut ports: Vec<UsbPortGroup> = port_counts
        .into_iter()
        .map(|(version, count)| UsbPortGroup { version, count })
        .collect();
    ports.sort_by(|a, b| a.version.cmp(&b.version));

    Ok(UsbSummary {
        ports,
        connected_devices: connected,
    })
}

/// Map nusb Speed enum to USB version string.
fn usb_speed_to_version(speed: Option<nusb::Speed>) -> String {
    match speed {
        Some(nusb::Speed::Low) => "1.0".to_string(),
        Some(nusb::Speed::Full) => "1.1".to_string(),
        Some(nusb::Speed::High) => "2.0".to_string(),
        Some(nusb::Speed::Super) => "3.0".to_string(),
        Some(nusb::Speed::SuperPlus) => "3.1".to_string(),
        // nusb may add more variants in future
        _ => "unknown".to_string(),
    }
}
