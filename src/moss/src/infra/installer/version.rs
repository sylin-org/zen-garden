//! Version breadcrumb for install tracking
//!
//! Records which version is installed, when, and how. Enables install-vs-update
//! detection and version delta display.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How the installation was triggered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMethod {
    /// `garden-moss install` (local)
    Install,
    /// `POST /api/v1/stone:deploy` (network)
    DeployApi,
    /// `garden-moss pre-start` (staged on restart)
    PreStart,
}

/// Persisted version breadcrumb written after every successful install/update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledVersion {
    pub version: String,
    pub installed_at: String,
    pub platform: String,
    pub method: InstallMethod,
}

impl InstalledVersion {
    pub fn new(version: &str, method: InstallMethod) -> Self {
        Self {
            version: version.to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            platform: current_platform().to_string(),
            method,
        }
    }
}

/// Read the installed version breadcrumb, if present
pub fn read_installed_version() -> Option<InstalledVersion> {
    let path = breadcrumb_path();
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Write the version breadcrumb after a successful install/update
pub fn write_installed_version(version: &InstalledVersion) -> anyhow::Result<()> {
    let path = breadcrumb_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(version)?;
    std::fs::write(&path, json)?;
    Ok(())
}

fn breadcrumb_path() -> PathBuf {
    Path::new(&garden_common::constants::paths::data_dir()).join("installed-version.json")
}

fn current_platform() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86"))]
    {
        "linux-x86"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

/// Detect whether this is a fresh install, update, or repair
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMode {
    /// No previous installation detected
    Fresh,
    /// Previous version found, upgrading
    Update { from: String, to: String },
    /// Same version, re-installing (repair)
    Repair { version: String },
}

impl InstallMode {
    /// Detect mode by comparing the installed breadcrumb against the current binary version
    pub fn detect(current_version: &str) -> Self {
        match read_installed_version() {
            None => InstallMode::Fresh,
            Some(installed) if installed.version == current_version => InstallMode::Repair {
                version: current_version.to_string(),
            },
            Some(installed) => InstallMode::Update {
                from: installed.version,
                to: current_version.to_string(),
            },
        }
    }
}

impl std::fmt::Display for InstallMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMode::Fresh => write!(f, "Fresh install"),
            InstallMode::Update { from, to } => write!(f, "Update ({from} -> {to})"),
            InstallMode::Repair { version } => write!(f, "Repair ({version})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_version_serializes_correctly() {
        let v = InstalledVersion {
            version: "0.2.202603161200".to_string(),
            installed_at: "2026-03-16T12:00:00Z".to_string(),
            platform: "linux-x64".to_string(),
            method: InstallMethod::Install,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"method\":\"install\""));
        assert!(json.contains("\"platform\":\"linux-x64\""));
    }

    #[test]
    fn installed_version_roundtrips() {
        let v = InstalledVersion {
            version: "0.2.202603161200".to_string(),
            installed_at: "2026-03-16T12:00:00Z".to_string(),
            platform: "linux-x64".to_string(),
            method: InstallMethod::DeployApi,
        };
        let json = serde_json::to_string_pretty(&v).unwrap();
        let parsed: InstalledVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, v.version);
        assert_eq!(parsed.method, InstallMethod::DeployApi);
    }

    #[test]
    fn install_mode_detect_returns_valid_mode() {
        // detect() reads from breadcrumb_path() which may or may not exist
        // in the test environment. We verify the result is structurally valid.
        let mode = InstallMode::detect("0.2.999");
        // If no breadcrumb exists, expect Fresh. If a breadcrumb was written by
        // a prior install, expect Update or Repair. Both are correct.
        match &mode {
            InstallMode::Fresh => {} // no breadcrumb on disk
            InstallMode::Update { from, to } => {
                assert!(!from.is_empty(), "update 'from' version must not be empty");
                assert_eq!(to, "0.2.999");
            }
            InstallMode::Repair { version } => {
                assert_eq!(version, "0.2.999");
            }
        }
    }

    #[test]
    fn install_mode_display() {
        assert_eq!(InstallMode::Fresh.to_string(), "Fresh install");
        assert_eq!(
            InstallMode::Update {
                from: "0.1".to_string(),
                to: "0.2".to_string()
            }
            .to_string(),
            "Update (0.1 -> 0.2)"
        );
        assert_eq!(
            InstallMode::Repair {
                version: "0.2".to_string()
            }
            .to_string(),
            "Repair (0.2)"
        );
    }
}
