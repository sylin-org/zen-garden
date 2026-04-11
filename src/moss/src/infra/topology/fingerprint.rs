//! PCI device fingerprint for delta-gated topology reprobing (ARCH-0014).
//!
//! Computes a SHA-256 hash of sorted PCI vendor:device ID pairs.
//! - Linux: enumerates sysfs `/sys/bus/pci/devices/*/vendor` + `device`
//! - Windows: enumerates `HKLM\SYSTEM\CurrentControlSet\Enum\PCI\` registry keys
//!
//! The fingerprint changes when PCI devices are added or removed (e.g., eGPU
//! hot-plug). If the fingerprint matches the cached value, the full topology
//! probe is skipped.

use sha2::{Digest, Sha256};

/// Compute a SHA-256 fingerprint of all PCI device IDs on this system.
///
/// Returns a hex-encoded hash string. On failure, returns an empty string
/// (which will never match a cached fingerprint, forcing a full probe).
pub async fn compute_fingerprint() -> String {
    match enumerate_pci_ids().await {
        Ok(mut ids) => {
            ids.sort();
            ids.dedup();
            let mut hasher = Sha256::new();
            for id in &ids {
                hasher.update(id.as_bytes());
                hasher.update(b"\n");
            }
            hex::encode(hasher.finalize())
        }
        Err(e) => {
            tracing::warn!(error = %e, "PCI fingerprint enumeration failed — forcing full probe");
            String::new()
        }
    }
}

/// Enumerate PCI vendor:device ID pairs.
///
/// Returns strings in "VVVV:DDDD" format (lowercase hex).
async fn enumerate_pci_ids() -> anyhow::Result<Vec<String>> {
    #[cfg(target_os = "linux")]
    {
        enumerate_pci_ids_linux().await
    }
    #[cfg(target_os = "windows")]
    {
        enumerate_pci_ids_windows().await
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(Vec::new())
    }
}

/// Linux: read vendor/device from sysfs — no process spawn, ~2ms.
#[cfg(target_os = "linux")]
async fn enumerate_pci_ids_linux() -> anyhow::Result<Vec<String>> {
    use tokio::fs;

    let pci_dir = "/sys/bus/pci/devices";
    let mut ids = Vec::new();

    let mut entries = fs::read_dir(pci_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        let vendor_path = path.join("vendor");
        let device_path = path.join("device");

        if let (Ok(vendor_raw), Ok(device_raw)) = (
            fs::read_to_string(&vendor_path).await,
            fs::read_to_string(&device_path).await,
        ) {
            // sysfs format: "0x10de\n" → strip prefix and trim
            let vendor = vendor_raw.trim().trim_start_matches("0x").to_lowercase();
            let device = device_raw.trim().trim_start_matches("0x").to_lowercase();
            if vendor.len() == 4 && device.len() == 4 {
                ids.push(format!("{vendor}:{device}"));
            }
        }
    }

    Ok(ids)
}

/// Windows: enumerate PCI registry keys — no process spawn, ~5-10ms.
///
/// Registry path: `HKLM\SYSTEM\CurrentControlSet\Enum\PCI\`
/// Each subkey is `VEN_XXXX&DEV_XXXX&SUBSYS_XXXXXXXX&REV_XX`.
/// We extract VEN and DEV to produce "xxxx:xxxx" pairs.
#[cfg(target_os = "windows")]
async fn enumerate_pci_ids_windows() -> anyhow::Result<Vec<String>> {
    // Registry reads are blocking — offload to a blocking thread.
    tokio::task::spawn_blocking(|| enumerate_pci_ids_windows_blocking()).await?
}

#[cfg(target_os = "windows")]
fn enumerate_pci_ids_windows_blocking() -> anyhow::Result<Vec<String>> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let pci_key = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\PCI")?;

    let mut ids = Vec::new();

    for subkey_name in pci_key.enum_keys().filter_map(|r| r.ok()) {
        // subkey_name format: "VEN_10DE&DEV_2684&SUBSYS_..."
        let name_upper = subkey_name.to_uppercase();
        let vendor = extract_between(&name_upper, "VEN_", "&");
        let device = extract_between(&name_upper, "DEV_", "&");

        if let (Some(v), Some(d)) = (vendor, device) {
            if v.len() == 4 && d.len() == 4 {
                ids.push(format!("{}:{}", v.to_lowercase(), d.to_lowercase()));
            }
        }
    }

    Ok(ids)
}

/// Extract substring between `prefix` and `suffix` in `s`.
#[cfg(target_os = "windows")]
fn extract_between<'a>(s: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = s.find(prefix)? + prefix.len();
    let rest = &s[start..];
    let end = rest.find(suffix).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_deterministic() {
        let mut ids = vec!["10de:2684".to_string(), "8086:a370".to_string()];
        ids.sort();
        let mut hasher = Sha256::new();
        for id in &ids {
            hasher.update(id.as_bytes());
            hasher.update(b"\n");
        }
        let hash1 = hex::encode(hasher.finalize());

        let mut ids2 = vec!["8086:a370".to_string(), "10de:2684".to_string()];
        ids2.sort();
        let mut hasher2 = Sha256::new();
        for id in &ids2 {
            hasher2.update(id.as_bytes());
            hasher2.update(b"\n");
        }
        let hash2 = hex::encode(hasher2.finalize());

        assert_eq!(hash1, hash2, "fingerprint must be order-independent");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_extract_between() {
        let s = "VEN_10DE&DEV_2684&SUBSYS_12345678&REV_A1";
        assert_eq!(extract_between(s, "VEN_", "&"), Some("10DE"));
        assert_eq!(extract_between(s, "DEV_", "&"), Some("2684"));
        assert_eq!(extract_between(s, "SUBSYS_", "&"), Some("12345678"));
        assert_eq!(extract_between(s, "REV_", "&"), Some("A1"));
    }
}
