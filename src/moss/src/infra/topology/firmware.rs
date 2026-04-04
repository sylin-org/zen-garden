//! Firmware inventory detection (ARCH-0014).
//!
//! Collects firmware versions for BIOS, SSDs, NICs, and other components.
//! - Linux: `dmidecode -t bios` + `fwupdmgr get-devices --json`
//! - Windows: `Win32_BIOS` + `MSFT_PhysicalDisk` + ESRT registry + PnP sweep

use anyhow::Result;
use garden_common::types::hardware_topology::FirmwareComponent;

/// Detect firmware versions across all components.
pub async fn detect_firmware() -> Result<Vec<FirmwareComponent>> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(detect_firmware_linux).await?
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(detect_firmware_windows).await?
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(Vec::new())
    }
}

// ── Linux ───────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn detect_firmware_linux() -> Result<Vec<FirmwareComponent>> {
    let mut components = Vec::new();

    // BIOS from SMBIOS (already parsed in smbios module, but firmware module
    // captures it independently for the firmware inventory view)
    if let Ok(smbios) = smbioslib::table_load_from_device() {
        for bios in smbios.collect::<smbioslib::SMBiosInformation>() {
            let vendor = bios.vendor().ok().unwrap_or_default();
            let version = bios.version().ok().unwrap_or_default();
            let date = bios.release_date().ok();

            if !version.is_empty() {
                components.push(FirmwareComponent {
                    component: "BIOS".to_string(),
                    vendor,
                    version,
                    date,
                    updatable: None,
                    device_name: None,
                });
            }
        }
    }

    // fwupdmgr for component-level firmware (if available)
    if let Ok(output) = std::process::Command::new("fwupdmgr")
        .args(["get-devices", "--json", "--no-unreported-check"])
        .output()
    {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(devices) = json.get("Devices").and_then(|d| d.as_array()) {
                    for device in devices {
                        let component = device
                            .get("Summary")
                            .and_then(|s| s.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let vendor = device
                            .get("Vendor")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let version = device
                            .get("Version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let updatable = device
                            .get("Flags")
                            .and_then(|f| f.as_array())
                            .map(|flags| {
                                flags.iter().any(|f| f.as_str() == Some("updatable"))
                            });
                        let device_name = device
                            .get("Name")
                            .and_then(|n| n.as_str())
                            .map(|n| n.to_string());

                        if !version.is_empty() {
                            components.push(FirmwareComponent {
                                component,
                                vendor,
                                version,
                                date: None,
                                updatable,
                                device_name,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(components)
}

// ── Windows ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn detect_firmware_windows() -> Result<Vec<FirmwareComponent>> {
    let mut components = Vec::new();

    // BIOS from SMBIOS
    if let Ok(smbios) = smbioslib::table_load_from_device() {
        for bios in smbios.collect::<smbioslib::SMBiosInformation>() {
            let vendor = bios.vendor().ok().unwrap_or_default();
            let version = bios.version().ok().unwrap_or_default();
            let date = bios.release_date().ok();

            if !version.is_empty() {
                components.push(FirmwareComponent {
                    component: "BIOS".to_string(),
                    vendor,
                    version,
                    date,
                    updatable: None,
                    device_name: None,
                });
            }
        }
    }

    // SSD firmware via WMI MSFT_PhysicalDisk
    if let Ok(wmi_con) = wmi::COMLibrary::new().and_then(|lib| wmi::WMIConnection::with_namespace_path(r"root\Microsoft\Windows\Storage", lib)) {
        #[derive(serde::Deserialize)]
        #[serde(rename = "MSFT_PhysicalDisk")]
        #[serde(rename_all = "PascalCase")]
        struct PhysicalDisk {
            friendly_name: Option<String>,
            firmware_version: Option<String>,
            media_type: Option<u16>,
        }

        if let Ok(disks) = wmi_con.query::<PhysicalDisk>() {
            for disk in disks {
                if let Some(fw) = disk.firmware_version {
                    if !fw.is_empty() {
                        let media = match disk.media_type {
                            Some(3) => "HDD",
                            Some(4) => "SSD",
                            _ => "Disk",
                        };
                        components.push(FirmwareComponent {
                            component: media.to_string(),
                            vendor: String::new(),
                            version: fw,
                            date: None,
                            updatable: None,
                            device_name: disk.friendly_name,
                        });
                    }
                }
            }
        }
    }

    // GPU driver version via WMI
    if let Ok(wmi_con) = wmi::COMLibrary::new().and_then(|lib| wmi::WMIConnection::new(lib)) {
        #[derive(serde::Deserialize)]
        #[serde(rename = "Win32_VideoController")]
        #[serde(rename_all = "PascalCase")]
        struct VideoController {
            name: Option<String>,
            driver_version: Option<String>,
        }

        if let Ok(gpus) = wmi_con.query::<VideoController>() {
            for gpu in gpus {
                if let Some(dv) = gpu.driver_version {
                    if !dv.is_empty() {
                        components.push(FirmwareComponent {
                            component: "GPU".to_string(),
                            vendor: String::new(),
                            version: dv,
                            date: None,
                            updatable: None,
                            device_name: gpu.name,
                        });
                    }
                }
            }
        }
    }

    Ok(components)
}
