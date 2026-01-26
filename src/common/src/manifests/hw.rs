//! Hardware Manifests
//!
//! Loads and stores hardware manifests for greenlit devices.
//! Each hardware manifest consists of:
//! - `{model}.manifest.yaml` - Hardware identity, firmware, profile
//! - `{model}.compatibility.yaml` - Service compatibility rules (optional)
//! - `{model}.frontmatter.json` - Metadata (optional)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::discover_subdirectories;

/// Runtime hardware manifests directory (platform-specific)
/// Maintains manifests/hw structure on all platforms
#[cfg(target_os = "linux")]
pub const RUNTIME_HW_MANIFESTS_DIR: &str = "/etc/zen-garden/manifests/hw";

#[cfg(target_os = "windows")]
pub const RUNTIME_HW_MANIFESTS_DIR: &str = ".zen-garden/manifests/hw";

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub const RUNTIME_HW_MANIFESTS_DIR: &str = ".zen-garden/manifests/hw";

/// Collection of all hardware manifests
#[derive(Debug)]
pub struct HwManifests {
    /// All hardware manifests keyed by "vendor/model" (e.g., "dell/wyse-5070")
    pub entries: HashMap<String, HwEntry>,
    /// Discovered vendor names, sorted alphabetically
    pub vendors: Vec<String>,
}

/// A single hardware manifest with all its data
#[derive(Debug, Clone)]
pub struct HwEntry {
    /// Vendor name (e.g., "dell")
    pub vendor: String,
    /// Model name (e.g., "wyse-5070")
    pub model: String,
    /// Raw manifest YAML content
    pub manifest_yaml: String,
    /// Parsed manifest data
    pub manifest: Option<HwManifestData>,
    /// Raw compatibility YAML content (if present)
    pub compatibility_yaml: Option<String>,
    /// Parsed frontmatter metadata (if present)
    pub frontmatter: Option<HwFrontmatter>,
}

/// Parsed hardware manifest data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwManifestData {
    pub name: String,
    pub vendor: String,
    #[serde(rename = "type")]
    pub manifest_type: String,
    pub identity: Option<HwIdentity>,
    pub firmware: Option<HwFirmware>,
    pub profile: Option<HwProfile>,
    pub bios: Option<HwBios>,
}

/// Hardware identity for detection
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwIdentity {
    pub system_manufacturer: Option<String>,
    pub system_product_name_patterns: Option<Vec<String>>,
    pub system_version_patterns: Option<Vec<String>>,
}

/// Firmware update configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwFirmware {
    pub method: String,
    pub lvfs_device_id: Option<String>,
    pub versions: Option<HwFirmwareVersions>,
    pub requires_reboot: Option<bool>,
    pub requires_ac_power: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwFirmwareVersions {
    pub minimum: Option<String>,
    pub recommended: Option<String>,
    pub latest_known: Option<String>,
}

/// Hardware profile affecting service placement
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwProfile {
    pub cpu_architecture: Option<String>,
    pub cpu_cores: Option<u32>,
    pub storage_type: Option<String>,
    pub storage_expandable: Option<bool>,
    pub fanless: Option<bool>,
    pub tdp_watts: Option<u32>,
    pub idle_watts: Option<u32>,
    pub max_watts: Option<u32>,
    pub form_factor: Option<String>,
}

/// BIOS access information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwBios {
    pub access_key: Option<String>,
    pub boot_menu_key: Option<String>,
    pub boot_mode: Option<String>,
}

/// Frontmatter metadata for a hardware manifest
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HwFrontmatter {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub release_year: Option<u32>,
    #[serde(default)]
    pub support_status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

impl HwManifests {
    /// Load all hardware manifests from a directory
    ///
    /// Scans all subdirectories (vendors) and loads manifests from each.
    /// Directory structure: `{dir}/{vendor}/{model}.manifest.yaml`
    pub fn load(dir: &Path) -> Result<Self> {
        let mut entries = HashMap::new();
        let mut vendors_set = std::collections::HashSet::new();

        if !dir.exists() {
            tracing::debug!(
                path = %dir.display(),
                "Hardware manifests directory not found"
            );
            return Ok(Self::empty());
        }

        // Discover vendors dynamically
        let vendors = discover_subdirectories(dir);

        for vendor in &vendors {
            let vendor_dir = dir.join(vendor);

            if let Ok(dir_entries) = std::fs::read_dir(&vendor_dir) {
                for entry in dir_entries.filter_map(Result::ok) {
                    let path = entry.path();

                    // Only process .manifest.yaml files
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".manifest.yaml") {
                            let model_name = name.trim_end_matches(".manifest.yaml");
                            let key = format!("{}/{}", vendor, model_name);

                            match Self::load_entry(&vendor_dir, vendor, model_name) {
                                Ok(hw_entry) => {
                                    vendors_set.insert(vendor.clone());
                                    entries.insert(key, hw_entry);
                                    tracing::debug!(
                                        vendor = vendor,
                                        model = model_name,
                                        "Loaded hardware manifest"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        vendor = vendor,
                                        model = model_name,
                                        error = %e,
                                        "Failed to load hardware manifest"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut vendors_vec: Vec<_> = vendors_set.into_iter().collect();
        vendors_vec.sort();

        Ok(Self {
            entries,
            vendors: vendors_vec,
        })
    }

    /// Load a single hardware entry from a vendor directory
    fn load_entry(vendor_dir: &Path, vendor: &str, model_name: &str) -> Result<HwEntry> {
        // Load manifest YAML (required)
        let manifest_path = vendor_dir.join(format!("{}.manifest.yaml", model_name));
        let manifest_yaml = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;

        // Parse manifest
        let manifest = match serde_yaml::from_str::<HwManifestData>(&manifest_yaml) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!(
                    vendor = vendor,
                    model = model_name,
                    error = %e,
                    "Failed to parse hardware manifest"
                );
                None
            }
        };

        // Load compatibility YAML (optional)
        let compat_path = vendor_dir.join(format!("{}.compatibility.yaml", model_name));
        let compatibility_yaml = if compat_path.exists() {
            std::fs::read_to_string(&compat_path).ok()
        } else {
            None
        };

        // Load frontmatter (optional)
        let frontmatter_path = vendor_dir.join(format!("{}.frontmatter.json", model_name));
        let frontmatter = if frontmatter_path.exists() {
            match std::fs::read_to_string(&frontmatter_path) {
                Ok(json) => {
                    let json = crate::utils::strings::strip_bom(&json);
                    serde_json::from_str::<HwFrontmatter>(json).ok()
                },
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(HwEntry {
            vendor: vendor.to_string(),
            model: model_name.to_string(),
            manifest_yaml,
            manifest,
            compatibility_yaml,
            frontmatter,
        })
    }

    /// Create an empty HwManifests
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
            vendors: Vec::new(),
        }
    }

    /// Get a hardware entry by "vendor/model" key
    pub fn get(&self, key: &str) -> Option<&HwEntry> {
        self.entries.get(key)
    }

    /// Get a hardware entry by vendor and model separately
    pub fn get_by_parts(&self, vendor: &str, model: &str) -> Option<&HwEntry> {
        self.entries.get(&format!("{}/{}", vendor, model))
    }

    /// Get all entries for a specific vendor
    pub fn by_vendor(&self, vendor: &str) -> Vec<&HwEntry> {
        self.entries
            .values()
            .filter(|e| e.vendor == vendor)
            .collect()
    }

    /// Get count of hardware manifests
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Find a hardware manifest that matches the given system identity
    /// Uses DMI/SMBIOS values from the running system
    pub fn find_matching(&self, manufacturer: Option<&str>, product: Option<&str>) -> Option<&HwEntry> {
        let mfr = manufacturer?;
        let prod = product?;
        
        self.entries.values().find(|entry| entry.matches_dmidecode(mfr, prod))
    }
}
impl HwEntry {
    /// Get the full key (vendor/model)
    pub fn key(&self) -> String {
        format!("{}/{}", self.vendor, self.model)
    }

    /// Check if this hardware matches the given dmidecode values
    pub fn matches_dmidecode(&self, manufacturer: &str, product_name: &str) -> bool {
        if let Some(ref manifest) = self.manifest {
            if let Some(ref identity) = manifest.identity {
                // Check manufacturer
                if let Some(ref mfr) = identity.system_manufacturer {
                    if !manufacturer.contains(mfr) && mfr != manufacturer {
                        return false;
                    }
                }

                // Check product name patterns
                if let Some(ref patterns) = identity.system_product_name_patterns {
                    for pattern in patterns {
                        if product_name.contains(pattern) {
                            return true;
                        }
                    }
                }

                // Check version patterns as fallback
                if let Some(ref patterns) = identity.system_version_patterns {
                    for pattern in patterns {
                        if product_name.contains(pattern) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_hw_manifest(dir: &Path, vendor: &str, model: &str) {
        let vendor_dir = dir.join(vendor);
        fs::create_dir_all(&vendor_dir).unwrap();

        fs::write(
            vendor_dir.join(format!("{}.manifest.yaml", model)),
            format!(
                r#"name: {}
vendor: {}
type: hardware
identity:
  system_manufacturer: "Test Inc."
  system_product_name_patterns:
    - "{}"
"#,
                model, vendor, model
            ),
        ).unwrap();
    }

    #[test]
    fn test_load_empty_directory() {
        let temp = TempDir::new().unwrap();
        let result = HwManifests::load(temp.path());
        assert!(result.is_ok());
        let manifests = result.unwrap();
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_load_hw_manifests() {
        let temp = TempDir::new().unwrap();
        create_test_hw_manifest(temp.path(), "dell", "wyse-5070");
        create_test_hw_manifest(temp.path(), "hp", "t630");

        let manifests = HwManifests::load(temp.path()).unwrap();

        assert_eq!(manifests.len(), 2);
        assert!(manifests.get("dell/wyse-5070").is_some());
        assert!(manifests.get("hp/t630").is_some());
        assert_eq!(manifests.vendors.len(), 2);
    }

    #[test]
    fn test_get_by_vendor() {
        let temp = TempDir::new().unwrap();
        create_test_hw_manifest(temp.path(), "dell", "wyse-5070");
        create_test_hw_manifest(temp.path(), "dell", "optiplex-3000");
        create_test_hw_manifest(temp.path(), "hp", "t630");

        let manifests = HwManifests::load(temp.path()).unwrap();

        let dell_entries = manifests.by_vendor("dell");
        assert_eq!(dell_entries.len(), 2);

        let hp_entries = manifests.by_vendor("hp");
        assert_eq!(hp_entries.len(), 1);
    }

    #[test]
    fn test_matches_dmidecode() {
        let entry = HwEntry {
            vendor: "dell".to_string(),
            model: "wyse-5070".to_string(),
            manifest_yaml: String::new(),
            manifest: Some(HwManifestData {
                name: "wyse-5070".to_string(),
                vendor: "dell".to_string(),
                manifest_type: "hardware".to_string(),
                identity: Some(HwIdentity {
                    system_manufacturer: Some("Dell Inc.".to_string()),
                    system_product_name_patterns: Some(vec!["Wyse 5070".to_string()]),
                    system_version_patterns: None,
                }),
                firmware: None,
                profile: None,
                bios: None,
            }),
            compatibility_yaml: None,
            frontmatter: None,
        };

        assert!(entry.matches_dmidecode("Dell Inc.", "Wyse 5070 Thin Client"));
        assert!(!entry.matches_dmidecode("HP Inc.", "t630"));
    }
}
