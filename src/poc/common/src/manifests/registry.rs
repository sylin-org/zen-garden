//! Unified Manifest Registry
//!
//! Single source of truth for all manifests - loaded once at startup.
//!
//! # Structure
//!
//! ```text
//! ManifestRegistry
//! ├── sw: OfferingRegistry          # All offerings (managed, adopted, borrowed)
//! │   ├── entries: HashMap          # Keyed by offering name (e.g., "mongodb")
//! │   └── categories: Vec           # Discovered category names
//! └── hw: HwManifests               # Hardware manifests
//!     ├── entries: HashMap          # Keyed by "vendor/model"
//!     └── vendors: Vec              # Discovered vendor names
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! // Load once at startup
//! let registry = ManifestRegistry::load(Path::new("/var/lib/zen-garden/manifests"))?;
//!
//! // Access offerings (all modes)
//! if let Some(offering) = registry.sw.get("mongodb") {
//!     let template = offering.parse_template()?;
//! }
//!
//! // Find adoptable offerings
//! for offering in registry.offerings_by_mode(&OfferingMode::Adopted) {
//!     // This offering supports adopted mode
//! }
//! ```

use crate::OfferingMode;
use crate::manifests::{HwManifests, Offering, OfferingRegistry};
use anyhow::{Context, Result};
use std::path::Path;

/// Runtime manifests directory
#[cfg(target_os = "linux")]
pub const RUNTIME_MANIFESTS_DIR: &str = "/var/lib/zen-garden/manifests";

#[cfg(target_os = "windows")]
pub const RUNTIME_MANIFESTS_DIR: &str = ".zen-garden/manifests";

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub const RUNTIME_MANIFESTS_DIR: &str = ".zen-garden/manifests";

/// Single source of truth for all manifests
///
/// Loaded once at startup, provides access to:
/// - Software offerings (all modes: managed, adopted, borrowed)
/// - Hardware manifests (device definitions)
#[derive(Debug)]
pub struct ManifestRegistry {
    /// All offerings (unified model)
    pub sw: OfferingRegistry,
    /// Hardware manifests (device definitions)
    pub hw: HwManifests,
}

impl ManifestRegistry {
    /// Load all manifests from the runtime directories
    pub fn load(sw_dir: &Path, hw_dir: Option<&Path>) -> Result<Self> {
        let sw = OfferingRegistry::load(sw_dir)
            .with_context(|| format!("Failed to load offerings from {}", sw_dir.display()))?;

        let hw = if let Some(dir) = hw_dir {
            HwManifests::load(dir).with_context(|| {
                format!("Failed to load hardware manifests from {}", dir.display())
            })?
        } else {
            HwManifests::empty()
        };

        tracing::info!(
            offerings = sw.entries.len(),
            categories = sw.categories.len(),
            hw_count = hw.entries.len(),
            "ManifestRegistry loaded"
        );

        Ok(Self { sw, hw })
    }

    /// Create an empty registry
    pub fn empty() -> Self {
        Self {
            sw: OfferingRegistry::empty(),
            hw: HwManifests::empty(),
        }
    }

    /// Create registry from pre-loaded OfferingRegistry
    pub fn from_sw_manifests(sw: OfferingRegistry, hw_dir: Option<&Path>) -> Result<Self> {
        let hw = if let Some(dir) = hw_dir {
            HwManifests::load(dir).with_context(|| {
                format!("Failed to load hardware manifests from {}", dir.display())
            })?
        } else {
            HwManifests::empty()
        };

        tracing::info!(
            offerings = sw.entries.len(),
            categories = sw.categories.len(),
            hw_count = hw.entries.len(),
            "ManifestRegistry created"
        );

        Ok(Self { sw, hw })
    }

    /// Get total count of all manifests
    pub fn total_count(&self) -> usize {
        self.sw.entries.len() + self.hw.entries.len()
    }

    /// Get all offerings that support a specific mode
    pub fn offerings_by_mode(&self, mode: &OfferingMode) -> Vec<&Offering> {
        self.sw.by_mode(mode)
    }

    /// Get offering by name
    pub fn get_offering(&self, name: &str) -> Option<&Offering> {
        self.sw.get(name)
    }

    /// Get mutable offering by name
    pub fn get_offering_mut(&mut self, name: &str) -> Option<&mut Offering> {
        self.sw.get_mut(name)
    }

    /// Insert or update an offering
    pub fn upsert_offering(&mut self, offering: Offering) -> bool {
        self.sw.upsert(offering)
    }
}

/// Discover subdirectories in a directory, skipping hidden and internal prefixes
pub fn discover_subdirectories(dir: &Path) -> Vec<String> {
    let mut subdirs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

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
    use std::fs;
    use tempfile::TempDir;

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

        fs::create_dir(temp.path().join("data")).unwrap();
        fs::create_dir(temp.path().join("cache")).unwrap();
        fs::create_dir(temp.path().join(".hidden")).unwrap();
        fs::create_dir(temp.path().join("_internal")).unwrap();
        fs::write(temp.path().join("file.txt"), "test").unwrap();

        let subdirs = discover_subdirectories(temp.path());

        assert_eq!(subdirs.len(), 2);
        assert!(subdirs.contains(&"cache".to_string()));
        assert!(subdirs.contains(&"data".to_string()));
    }

    #[test]
    fn test_discover_subdirectories_nonexistent() {
        let subdirs = discover_subdirectories(Path::new("/nonexistent/path/12345"));
        assert_eq!(subdirs.len(), 0);
    }
}
