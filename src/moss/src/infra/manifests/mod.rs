//! Unified Manifest Registry
//!
//! Single source of truth for all manifests - loaded once at startup.
//! Replaces the fragmented TemplateLoader and manifest_loader approach.
//!
//! # Structure
//!
//! ```text
//! ManifestRegistry
//! ├── sw: SwManifests              # Software offerings (container templates)
//! │   ├── entries: HashMap         # Keyed by offering name (e.g., "mongodb")
//! │   └── categories: Vec          # Discovered category names
//! ├── hw: HwManifests              # Hardware manifests
//! │   ├── entries: HashMap         # Keyed by "vendor/model" (e.g., "dell/wyse-5070")
//! │   └── vendors: Vec             # Discovered vendor names
//! └── offering_manifests: HashMap  # Multi-mode offering definitions (adoption/borrowing)
//!     └── OfferingManifest         # Detection rules, control commands, etc.
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! // Load once at startup
//! let registry = ManifestRegistry::load(Path::new("/etc/zen-garden/templates"))?;
//!
//! // Access software offerings
//! if let Some(entry) = registry.sw.get("mongodb") {
//!     let template = entry.parse_template()?;
//! }
//!
//! // List all offerings
//! for entry in registry.sw.entries.values() {
//!     println!("{}: {}", entry.name, entry.category);
//! }
//!
//! // Check for adoptable offerings
//! for manifest in registry.offering_manifests.values() {
//!     if manifest.modes.contains(&OfferingMode::Adopted) {
//!         // This offering can be adopted
//!     }
//! }
//! ```

mod sw;
mod hw;

pub use sw::{SwManifests, SwEntry, SwFrontmatter, ServiceTemplate, TemplateInfo, RUNTIME_TEMPLATES_DIR};
pub use hw::{HwManifests, HwEntry, HwFrontmatter, RUNTIME_HW_MANIFESTS_DIR};

use anyhow::{Context, Result};
use garden_common::manifests::OfferingManifest;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

/// Runtime manifests directory for multi-mode offerings (adoption/borrowing)
#[cfg(target_os = "windows")]
pub const RUNTIME_MANIFESTS_DIR: &str = "C:\\ProgramData\\ZenGarden\\manifests";

#[cfg(not(target_os = "windows"))]
pub const RUNTIME_MANIFESTS_DIR: &str = "/etc/zen-garden/manifests";

/// Single source of truth for all manifests
///
/// Loaded once at startup, provides access to:
/// - Software offerings (container templates)
/// - Hardware manifests (device definitions)
/// - Offering manifests (multi-mode definitions for adoption/borrowing)
#[derive(Debug)]
pub struct ManifestRegistry {
    /// Software offering manifests (container templates)
    pub sw: SwManifests,
    /// Hardware manifests (device definitions)
    pub hw: HwManifests,
    /// Multi-mode offering manifests (adoption/borrowing definitions)
    pub offering_manifests: HashMap<String, OfferingManifest>,
}

impl ManifestRegistry {
    /// Load all manifests from the runtime directories
    ///
    /// Scans directories once and loads:
    /// - Software templates from `sw_dir/{category}/*.snippet.yaml`
    /// - Hardware manifests from `hw_dir/{vendor}/*.manifest.yaml`
    /// - Offering manifests from `manifests_dir/{category}/*.manifest.yaml`
    ///
    /// # Arguments
    /// * `sw_dir` - Directory containing software templates (e.g., /etc/zen-garden/templates)
    /// * `hw_dir` - Directory containing hardware manifests (e.g., /var/lib/zen-garden/hw-manifests)
    pub fn load(sw_dir: &Path, hw_dir: Option<&Path>) -> Result<Self> {
        let sw = SwManifests::load(sw_dir)
            .with_context(|| format!("Failed to load software manifests from {}", sw_dir.display()))?;

        let hw = if let Some(dir) = hw_dir {
            HwManifests::load(dir)
                .with_context(|| format!("Failed to load hardware manifests from {}", dir.display()))?
        } else {
            HwManifests::empty()
        };

        // Load offering manifests (for adoption/borrowing)
        let manifests_dir = Path::new(RUNTIME_MANIFESTS_DIR);
        let offering_manifests = Self::load_offering_manifests(manifests_dir);

        tracing::info!(
            sw_count = sw.entries.len(),
            sw_categories = sw.categories.len(),
            hw_count = hw.entries.len(),
            hw_vendors = hw.vendors.len(),
            offering_count = offering_manifests.len(),
            "ManifestRegistry loaded"
        );

        Ok(Self { sw, hw, offering_manifests })
    }

    /// Load offering manifests for multi-mode offerings (adoption/borrowing)
    fn load_offering_manifests(dir: &Path) -> HashMap<String, OfferingManifest> {
        let mut manifests = HashMap::new();

        if !dir.exists() {
            tracing::debug!(
                path = %dir.display(),
                "Offering manifests directory not found"
            );
            return manifests;
        }

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories and non-YAML files
            if !path.is_file() {
                continue;
            }

            let extension = path.extension().and_then(|s| s.to_str());
            if !matches!(extension, Some("yaml") | Some("yml")) {
                continue;
            }

            // Load and parse the manifest
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    match serde_yaml::from_str::<OfferingManifest>(&content) {
                        Ok(manifest) => {
                            // Basic validation
                            if manifest.name.is_empty() || manifest.category.is_empty() || manifest.modes.is_empty() {
                                tracing::warn!(
                                    path = %path.display(),
                                    "Skipping invalid offering manifest (missing name, category, or modes)"
                                );
                                continue;
                            }
                            tracing::debug!(
                                name = %manifest.name,
                                path = %path.display(),
                                modes = ?manifest.modes,
                                "Loaded offering manifest"
                            );
                            manifests.insert(manifest.name.clone(), manifest);
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to parse offering manifest"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to read offering manifest"
                    );
                }
            }
        }

        manifests
    }

    /// Create an empty registry (for testing or when no manifests exist)
    pub fn empty() -> Self {
        Self {
            sw: SwManifests::empty(),
            hw: HwManifests::empty(),
            offering_manifests: HashMap::new(),
        }
    }

    /// Get total count of all manifests
    pub fn total_count(&self) -> usize {
        self.sw.entries.len() + self.hw.entries.len() + self.offering_manifests.len()
    }

    /// Get an offering manifest by name
    pub fn get_offering_manifest(&self, name: &str) -> Option<&OfferingManifest> {
        self.offering_manifests.get(name)
    }

    /// Get all offering manifests that support a specific mode
    pub fn offerings_by_mode(&self, mode: &garden_common::OfferingMode) -> Vec<&OfferingManifest> {
        self.offering_manifests
            .values()
            .filter(|m| m.modes.contains(mode))
            .collect()
    }
}

/// Discover subdirectories in a directory, skipping hidden and internal prefixes
///
/// Used by both sw and hw to discover categories/vendors dynamically.
pub(crate) fn discover_subdirectories(dir: &Path) -> Vec<String> {
    let mut subdirs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Include only directories, skip hidden (.) and internal (_) prefixes
            if path.is_dir() && !name.starts_with('.') && !name.starts_with('_') {
                subdirs.push(name);
            }
        }
    }

    subdirs.sort();
    subdirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_empty_registry() {
        let registry = ManifestRegistry::empty();
        assert_eq!(registry.sw.entries.len(), 0);
        assert_eq!(registry.hw.entries.len(), 0);
        assert_eq!(registry.total_count(), 0);
    }

    #[test]
    fn test_discover_subdirectories() {
        let temp = TempDir::new().unwrap();

        // Create some directories
        fs::create_dir(temp.path().join("data")).unwrap();
        fs::create_dir(temp.path().join("cache")).unwrap();
        fs::create_dir(temp.path().join(".hidden")).unwrap();
        fs::create_dir(temp.path().join("_internal")).unwrap();

        // Create a file (should be ignored)
        fs::write(temp.path().join("file.txt"), "test").unwrap();

        let subdirs = discover_subdirectories(temp.path());

        assert_eq!(subdirs.len(), 2);
        assert!(subdirs.contains(&"cache".to_string()));
        assert!(subdirs.contains(&"data".to_string()));
        assert!(!subdirs.contains(&".hidden".to_string()));
        assert!(!subdirs.contains(&"_internal".to_string()));
    }

    #[test]
    fn test_discover_subdirectories_nonexistent() {
        let subdirs = discover_subdirectories(Path::new("/nonexistent/path/12345"));
        assert_eq!(subdirs.len(), 0);
    }
}
