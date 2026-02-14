//! Firmware update detection and management
//!
//! Platform-specific firmware update detection:
//! - Linux: fwupd/LVFS integration
//! - Windows: Stub for V0 (future: Windows Update API)

#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Firmware update information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareUpdate {
    pub device_id: String,
    pub device_name: String,
    pub vendor: String,
    pub current_version: String,
    pub available_version: String,
    pub requires_reboot: bool,
    pub description: Option<String>,
}

/// Detect available firmware updates
///
/// Platform behavior:
/// - Linux: Query fwupd via D-Bus or fwupdmgr CLI
/// - Windows: Returns empty list (stub for V0)
pub async fn detect_firmware_updates() -> Result<Vec<FirmwareUpdate>> {
    #[cfg(target_os = "linux")]
    {
        detect_firmware_updates_linux().await
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Windows stub for V0 - future implementation can use Windows Update API
        tracing::debug!("Firmware detection not implemented for this platform");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "linux")]
async fn detect_firmware_updates_linux() -> Result<Vec<FirmwareUpdate>> {
    use tokio::process::Command;

    // Check if fwupdmgr is available
    let fwupd_check = Command::new("which").arg("fwupdmgr").output().await;

    if fwupd_check.is_err() || !fwupd_check.unwrap().status.success() {
        tracing::debug!("fwupdmgr not found - skipping firmware detection");
        return Ok(Vec::new());
    }

    // Refresh firmware metadata (non-blocking, may fail if offline)
    let _ = Command::new("fwupdmgr")
        .arg("refresh")
        .arg("--force")
        .output()
        .await;

    // Get list of updates
    let output = Command::new("fwupdmgr")
        .arg("get-updates")
        .arg("--json")
        .output()
        .await
        .context("Failed to execute fwupdmgr get-updates")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // No updates available is not an error
        if stderr.contains("No updates available") || stderr.contains("no updatable devices") {
            tracing::debug!("No firmware updates available");
            return Ok(Vec::new());
        }

        anyhow::bail!("fwupdmgr get-updates failed: {}", stderr);
    }

    // Parse JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_fwupd_json(&stdout)
}

#[cfg(target_os = "linux")]
fn parse_fwupd_json(json_str: &str) -> Result<Vec<FirmwareUpdate>> {
    use serde_json::Value;

    let data: Value =
        serde_json::from_str(json_str).context("Failed to parse fwupdmgr JSON output")?;

    let mut updates = Vec::new();

    // fwupdmgr JSON structure: { "Devices": [ { "DeviceId": "...", "Releases": [...] } ] }
    if let Some(devices) = data.get("Devices").and_then(|d| d.as_array()) {
        for device in devices {
            let device_id: String = device
                .get("DeviceId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let device_name: String = device
                .get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Device")
                .to_string();

            let vendor: String = device
                .get("Vendor")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let current_version: String = device
                .get("Version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .to_string();

            // Get first available release
            if let Some(releases) = device.get("Releases").and_then(|r| r.as_array()) {
                if let Some(release) = releases.first() {
                    let available_version: String = release
                        .get("Version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let description: Option<String> = release
                        .get("Description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    // Most firmware updates require reboot
                    let requires_reboot = true;

                    updates.push(FirmwareUpdate {
                        device_id,
                        device_name,
                        vendor,
                        current_version,
                        available_version,
                        requires_reboot,
                        description,
                    });
                }
            }
        }
    }

    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_fwupd_json() {
        let json = r#"{
            "Devices": [
                {
                    "DeviceId": "com.dell.bios",
                    "Name": "System BIOS",
                    "Vendor": "Dell Inc.",
                    "Version": "1.2.3",
                    "Releases": [
                        {
                            "Version": "1.2.4",
                            "Description": "Security fixes and improvements"
                        }
                    ]
                }
            ]
        }"#;

        let updates = parse_fwupd_json(json).unwrap();
        assert_eq!(updates.len(), 1);

        let update = &updates[0];
        assert_eq!(update.device_id, "com.dell.bios");
        assert_eq!(update.device_name, "System BIOS");
        assert_eq!(update.vendor, "Dell Inc.");
        assert_eq!(update.current_version, "1.2.3");
        assert_eq!(update.available_version, "1.2.4");
        assert!(update.requires_reboot);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_fwupd_json_no_updates() {
        let json = r#"{ "Devices": [] }"#;
        let updates = parse_fwupd_json(json).unwrap();
        assert_eq!(updates.len(), 0);
    }

    #[tokio::test]
    async fn test_detect_firmware_updates_non_linux() {
        #[cfg(not(target_os = "linux"))]
        {
            let updates = detect_firmware_updates().await.unwrap();
            assert_eq!(updates.len(), 0);
        }
    }
}
