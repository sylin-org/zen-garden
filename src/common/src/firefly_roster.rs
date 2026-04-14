//! Shared firefly roster data model (FIREFLY-0004).
//!
//! The roster is the host-side inventory of provisioned firefly
//! devices. Two parties read/write it:
//!
//! - **Operator machine** (`installer/FireflyDeviceId.psm1`): appends
//!   an entry whenever `newfirefly.ps1` mints a GUIDv7 and flashes a
//!   device. Lives at [`paths::operator_firefly_roster`].
//! - **Stone** (synced via `garden-rake firefly roster push <stone>`):
//!   mirrors the operator roster to [`paths::stone_firefly_roster`]
//!   so moss can surface labels and provenance in telemetry.
//!
//! Schema documented in `docs/specs/firefly-device-protocol.md`.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A provisioning entry recorded the moment a device receives its
/// GUIDv7 via `newfirefly.ps1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FireflyRosterEntry {
    /// GUIDv7 minted at provisioning time and burned into firmware.
    pub device_id: String,
    /// ISO-8601 UTC timestamp of the mint (`2026-04-14T15:30:00Z`).
    pub minted_at: String,
    /// `user@host` record of who minted this device.
    pub minted_by: String,
    /// Device variant — `matrix`, `oled-v1`, `oled-v2`, `tdisplay`.
    pub variant: String,
    /// Operator-assigned human label. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Firmware version flashed at provisioning time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_version_at_provisioning: Option<String>,
    /// Stone this device is intended for. Optional metadata only —
    /// the device itself can be plugged into any stone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stone_assigned_to: Option<String>,
}

/// Top-level roster file shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireflyRoster {
    /// Schema version. Bumped only on a breaking change; additive
    /// fields are ignored by older consumers.
    #[serde(default = "default_schema_version")]
    pub version: u32,
    /// Historical record of every provisioning event. Re-minting the
    /// same physical device appends a new entry; prior entries are
    /// preserved as history.
    #[serde(default)]
    pub fireflies: Vec<FireflyRosterEntry>,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for FireflyRoster {
    fn default() -> Self {
        Self {
            version: 1,
            fireflies: Vec::new(),
        }
    }
}

impl FireflyRoster {
    /// Load from a path. Missing file → empty roster. Malformed file
    /// → `Err` (callers decide whether to warn-and-continue or abort).
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        match std::fs::read_to_string(path.as_ref()) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Write-then-rename persistence for crash safety.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = {
            let mut buf = path.to_path_buf();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("firefly-roster.json");
            buf.set_file_name(format!(".{name}.tmp"));
            buf
        };
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Look up an entry by `device_id`. Returns the most recent entry
    /// if multiple entries share the id (re-provisioning history).
    pub fn find(&self, device_id: &str) -> Option<&FireflyRosterEntry> {
        self.fireflies.iter().rev().find(|e| e.device_id == device_id)
    }

    /// Distinct device_ids currently in the roster.
    pub fn device_count(&self) -> usize {
        use std::collections::HashSet;
        let set: HashSet<&str> = self
            .fireflies
            .iter()
            .map(|e| e.device_id.as_str())
            .collect();
        set.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_entry(device_id: &str, label: Option<&str>) -> FireflyRosterEntry {
        FireflyRosterEntry {
            device_id: device_id.to_string(),
            minted_at: "2026-04-14T15:30:00Z".to_string(),
            minted_by: "leo@workstation".to_string(),
            variant: "oled-v2".to_string(),
            label: label.map(String::from),
            firmware_version_at_provisioning: Some("2.0.0".to_string()),
            stone_assigned_to: None,
        }
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let r = FireflyRoster::load(dir.path().join("absent.json")).unwrap();
        assert!(r.fireflies.is_empty());
        assert_eq!(r.version, 1);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("roster.json");
        let roster = FireflyRoster {
            version: 1,
            fireflies: vec![sample_entry("01938abc", Some("garage"))],
        };
        roster.save(&path).unwrap();
        let loaded = FireflyRoster::load(&path).unwrap();
        assert_eq!(loaded.fireflies.len(), 1);
        assert_eq!(loaded.fireflies[0].device_id, "01938abc");
        assert_eq!(loaded.fireflies[0].label.as_deref(), Some("garage"));
    }

    #[test]
    fn find_returns_latest_entry_for_duplicate_ids() {
        let roster = FireflyRoster {
            version: 1,
            fireflies: vec![
                sample_entry("01938abc", Some("old-label")),
                sample_entry("01938abc", Some("new-label")),
            ],
        };
        assert_eq!(
            roster.find("01938abc").unwrap().label.as_deref(),
            Some("new-label")
        );
    }

    #[test]
    fn device_count_dedups_by_device_id() {
        let roster = FireflyRoster {
            version: 1,
            fireflies: vec![
                sample_entry("01938abc", None),
                sample_entry("01938abc", Some("re-minted")),
                sample_entry("01938def", None),
            ],
        };
        assert_eq!(roster.device_count(), 2);
    }

    #[test]
    fn malformed_json_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("roster.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(FireflyRoster::load(&path).is_err());
    }
}
