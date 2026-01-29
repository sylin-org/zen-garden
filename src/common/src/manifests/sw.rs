//! Software Offering Manifests
//!
//! Loads and stores software offering manifests from the manifests directory.
//! Each offering consists of:
//! - `{name}.snippet.yaml` - Container definition (image, ports, env, volumes)
//! - `{name}.compatibility.yaml` - Hardware compatibility rules (optional)
//! - `{name}.frontmatter.json` - Metadata (description, category, tags)

use anyhow::{Context, Result};
use crate::CompatibilityRules;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::discover_subdirectories;

/// Get runtime manifests directory (uses platform-aware paths)
pub fn runtime_manifests_dir() -> String {
    // 1. Check explicit override (deployment/testing)
    if let Ok(dir) = std::env::var("GARDEN_MANIFESTS_DIR") {
        return dir;
    }
    
    // 2. Check production location ({data_dir}/manifests)
    // At runtime, Moss extracts embedded manifests to this location
    let production_dir = format!("{}/manifests", crate::constants::paths::data_dir());
    if std::path::Path::new(&production_dir).exists() {
        return production_dir;
    }
    
    // 3. Dev fallback: Check embedded manifests location in repo
    if let Ok(current_dir) = std::env::current_dir() {
        // Common dev paths for embedded manifests (relative to cargo workspace)
        let dev_paths = [
            "src/moss/embedded/manifests",
            "../moss/embedded/manifests",
            "../../moss/embedded/manifests",
            "../../../moss/embedded/manifests",
        ];
        
        for relative_path in dev_paths {
            let path = current_dir.join(relative_path);
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
    }
    
    // 4. Fallback to production path (will fail gracefully if not found)
    production_dir
}

// ============================================================================
// Service Template Types
// ============================================================================

/// Parsed service template ready for container creation
#[derive(Debug, Clone)]
pub struct ServiceTemplate {
    pub image: String,
    pub ports: Vec<(u16, u16)>,              // (host_port, container_port)
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,       // (host_path, container_path)
    pub compatibility: Option<CompatibilityRules>,
}

/// Template listing info (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

// Internal: YAML parsing structures
#[derive(Debug, Deserialize)]
struct ComposeFile {
    services: HashMap<String, ServiceConfig>,
}

#[derive(Debug, Deserialize, Clone)]
struct ServiceConfig {
    image: String,
    #[serde(default)]
    ports: Vec<(u16, u16)>,
    #[serde(default)]
    environment: Option<serde_yaml::Value>,
    #[serde(default)]
    volumes: Vec<String>,
}

// ============================================================================
// SwManifests
// ============================================================================

/// Collection of all software offering manifests
#[derive(Debug)]
pub struct SwManifests {
    /// All offerings keyed by name (e.g., "mongodb", "redis")
    pub entries: HashMap<String, SwEntry>,
    /// Discovered category names, sorted alphabetically
    pub categories: Vec<String>,
}

/// A single software offering with all its manifest data
#[derive(Debug, Clone)]
pub struct SwEntry {
    /// Offering name (e.g., "mongodb")
    pub name: String,
    /// Category (e.g., "data", "cache")
    pub category: String,
    /// Raw snippet YAML content
    pub snippet_yaml: String,
    /// Parsed compatibility rules (if present)
    pub compatibility: Option<CompatibilityRules>,
    /// Parsed frontmatter metadata (if present)
    pub frontmatter: Option<SwFrontmatter>,
}

/// Frontmatter metadata for a software offering
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SwFrontmatter {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
}

impl SwManifests {
    /// Load all software manifests from a directory
    ///
    /// Scans all subdirectories (categories) and loads offerings from each.
    /// Directory structure: `{dir}/{category}/{offering}.snippet.yaml`
    pub fn load(dir: &Path) -> Result<Self> {
        let mut entries = HashMap::new();
        let mut categories_set = std::collections::HashSet::new();

        if !dir.exists() {
            tracing::warn!(
                path = %dir.display(),
                "Software manifests directory not found"
            );
            return Ok(Self::empty());
        }

        // Discover categories dynamically
        let categories = discover_subdirectories(dir);

        for category in &categories {
            let category_dir = dir.join(category);

            if let Ok(dir_entries) = std::fs::read_dir(&category_dir) {
                for entry in dir_entries.filter_map(Result::ok) {
                    let path = entry.path();

                    // Only process .snippet.yaml files
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".snippet.yaml") {
                            let offering_name = name.trim_end_matches(".snippet.yaml");

                            match Self::load_entry(&category_dir, category, offering_name) {
                                Ok(sw_entry) => {
                                    categories_set.insert(sw_entry.category.clone());
                                    entries.insert(offering_name.to_string(), sw_entry);
                                    tracing::debug!(
                                        offering = offering_name,
                                        category = category,
                                        "Loaded software manifest"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        offering = offering_name,
                                        category = category,
                                        error = %e,
                                        "Failed to load software manifest"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also check root level for any offerings not in categories
        if let Ok(dir_entries) = std::fs::read_dir(dir) {
            for entry in dir_entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".snippet.yaml") {
                            let offering_name = name.trim_end_matches(".snippet.yaml");
                            if !entries.contains_key(offering_name) {
                                match Self::load_entry(dir, "uncategorized", offering_name) {
                                    Ok(sw_entry) => {
                                        categories_set.insert(sw_entry.category.clone());
                                        entries.insert(offering_name.to_string(), sw_entry);
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            offering = offering_name,
                                            error = %e,
                                            "Failed to load root-level software manifest"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut categories_vec: Vec<_> = categories_set.into_iter().collect();
        categories_vec.sort();

        Ok(Self {
            entries,
            categories: categories_vec,
        })
    }

    /// Load a single offering entry from a category directory
    fn load_entry(category_dir: &Path, category: &str, offering_name: &str) -> Result<SwEntry> {
        // Load snippet YAML (required)
        let snippet_path = category_dir.join(format!("{}.snippet.yaml", offering_name));
        let snippet_yaml = std::fs::read_to_string(&snippet_path)
            .with_context(|| format!("Failed to read snippet: {}", snippet_path.display()))?;

        // Load compatibility rules (optional)
        let compat_path = category_dir.join(format!("{}.compatibility.yaml", offering_name));
        let compatibility = if compat_path.exists() {
            match std::fs::read_to_string(&compat_path) {
                Ok(yaml) => match serde_yaml::from_str::<CompatibilityRules>(&yaml) {
                    Ok(rules) => Some(rules),
                    Err(e) => {
                        tracing::warn!(
                            offering = offering_name,
                            path = %compat_path.display(),
                            error = %e,
                            "Failed to parse compatibility rules"
                        );
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        offering = offering_name,
                        error = %e,
                        "Failed to read compatibility file"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Load frontmatter (optional)
        let frontmatter_path = category_dir.join(format!("{}.frontmatter.json", offering_name));
        let frontmatter = if frontmatter_path.exists() {
            match std::fs::read_to_string(&frontmatter_path) {
                Ok(json) => {
                    // Strip UTF-8 BOM if present (Windows issue)
                    let json = crate::utils::strings::strip_bom(&json);
                    match serde_json::from_str::<SwFrontmatter>(json) {
                        Ok(fm) => Some(fm),
                        Err(e) => {
                            tracing::warn!(
                                offering = offering_name,
                                path = ?frontmatter_path,
                                error = %e,
                                "Failed to parse frontmatter"
                            );
                            None
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        offering = offering_name,
                        error = %e,
                        "Failed to read frontmatter file"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Determine effective category (frontmatter can override directory)
        let effective_category = frontmatter
            .as_ref()
            .and_then(|f| f.category.clone())
            .unwrap_or_else(|| category.to_string());

        Ok(SwEntry {
            name: offering_name.to_string(),
            category: effective_category,
            snippet_yaml,
            compatibility,
            frontmatter,
        })
    }

    /// Create an empty SwManifests (for testing or when directory doesn't exist)
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
            categories: Vec::new(),
        }
    }

    /// Get an offering by name
    pub fn get(&self, name: &str) -> Option<&SwEntry> {
        self.entries.get(name)
    }

    /// Check if an offering exists
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Get all offerings in a specific category
    pub fn by_category(&self, category: &str) -> Vec<&SwEntry> {
        self.entries
            .values()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Get count of offerings
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Insert or update an entry (for overlay pattern)
    /// 
    /// Returns true if this was an update (entry already existed), false if new
    pub fn upsert_entry(&mut self, entry: SwEntry) -> bool {
        let existed = self.entries.contains_key(&entry.name);
        
        // Update categories if needed
        if !self.categories.contains(&entry.category) {
            self.categories.push(entry.category.clone());
            self.categories.sort();
        }
        
        self.entries.insert(entry.name.clone(), entry);
        existed
    }
    
    /// Load entry from raw content (for embedded assets)
    /// 
    /// This allows loading without filesystem access - the caller provides:
    /// - relative_path: e.g., "sw/data/mongodb.snippet.yaml"
    /// - snippet_content: the raw YAML content
    /// - compatibility_content: optional compatibility YAML
    /// - frontmatter_content: optional frontmatter JSON
    pub fn load_entry_from_content(
        relative_path: &str,
        snippet_content: &str,
        compatibility_content: Option<&str>,
        frontmatter_content: Option<&str>,
    ) -> Result<SwEntry> {
        // Parse path: "sw/{category}/{name}.snippet.yaml"
        let parts: Vec<&str> = relative_path.split('/').collect();
        if parts.len() < 3 {
            anyhow::bail!("Invalid manifest path format: {}", relative_path);
        }
        
        // Extract category and name from path
        let category = parts[1].to_string();
        let filename = parts.last().unwrap();
        let name = filename.trim_end_matches(".snippet.yaml").to_string();
        
        // Parse compatibility (optional)
        let compatibility = compatibility_content.and_then(|yaml| {
            match serde_yaml::from_str::<CompatibilityRules>(yaml) {
                Ok(rules) => Some(rules),
                Err(e) => {
                    tracing::warn!(
                        offering = %name,
                        error = %e,
                        "Failed to parse embedded compatibility rules"
                    );
                    None
                }
            }
        });
        
        // Parse frontmatter (optional)
        let frontmatter = frontmatter_content.and_then(|json| {
            let json = crate::utils::strings::strip_bom(json);
            match serde_json::from_str::<SwFrontmatter>(json) {
                Ok(fm) => Some(fm),
                Err(e) => {
                    tracing::warn!(
                        offering = %name,
                        error = %e,
                        "Failed to parse embedded frontmatter"
                    );
                    None
                }
            }
        });
        
        Ok(SwEntry {
            name,
            category,
            snippet_yaml: snippet_content.to_string(),
            compatibility,
            frontmatter,
        })
    }
}

impl SwEntry {
    /// Get description from frontmatter or generate default
    pub fn description(&self) -> String {
        self.frontmatter
            .as_ref()
            .and_then(|f| f.description.clone())
            .unwrap_or_else(|| format!("{} service", self.name))
    }

    /// Get tags from frontmatter, normalized to lowercase
    pub fn tags(&self) -> Vec<String> {
        self.frontmatter
            .as_ref()
            .and_then(|f| f.tags.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Convert to TemplateInfo for API responses
    pub fn to_template_info(&self) -> TemplateInfo {
        TemplateInfo {
            name: self.name.clone(),
            category: self.category.clone(),
            description: self.description(),
            tags: self.tags(),
        }
    }

    /// Parse snippet YAML into ServiceTemplate
    ///
    /// Supports both snippet format (direct service config) and
    /// legacy compose format (with services: wrapper).
    pub fn parse_template(&self) -> Result<ServiceTemplate> {
        let yaml = self.snippet_yaml.replace("\r\n", "\n");

        // Try parsing as snippet format first (direct service config)
        if let Ok(service_config) = serde_yaml::from_str::<ServiceConfig>(&yaml) {
            return Ok(Self::service_config_to_template(
                service_config,
                self.compatibility.clone(),
            ));
        }

        // Fallback: try parsing as compose file (legacy format)
        let compose: ComposeFile = serde_yaml::from_str(&yaml)
            .with_context(|| format!(
                "Failed to parse YAML for '{}'. First 100 chars: {}",
                self.name,
                &yaml[..yaml.len().min(100)]
            ))?;

        let service_config = compose
            .services
            .get(&self.name)
            .with_context(|| format!("Service '{}' not found in compose file", self.name))?
            .clone();

        Ok(Self::service_config_to_template(
            service_config,
            self.compatibility.clone(),
        ))
    }

    /// Convert ServiceConfig to ServiceTemplate
    fn service_config_to_template(
        config: ServiceConfig,
        compatibility: Option<CompatibilityRules>,
    ) -> ServiceTemplate {
        // Ports are already tuples (host, container)
        let ports = config.ports;

        // Parse environment variables (support both list and map formats)
        let environment = match &config.environment {
            Some(serde_yaml::Value::Sequence(list)) => {
                list.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            }
            Some(serde_yaml::Value::Mapping(map)) => {
                map.iter()
                    .filter_map(|(k, v)| {
                        let key = k.as_str()?;
                        let value = v.as_str().unwrap_or("");
                        Some(format!("{}={}", key, value))
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        // Parse volumes (format: "host:container" or "volume_name:container")
        let volumes = config
            .volumes
            .iter()
            .filter_map(|v| {
                let parts: Vec<&str> = v.split(':').collect();
                if parts.len() >= 2 {
                    let host_path = if parts[0].starts_with('/') || parts[0].contains('\\') {
                        parts[0].to_string()
                    } else {
                        // Named volume: use platform-specific base path
                        #[cfg(target_os = "windows")]
                        let base = "C:\\ProgramData\\ZenGarden\\volumes";
                        #[cfg(not(target_os = "windows"))]
                        let base = "/var/lib/zen-garden/volumes";

                        format!("{}/{}", base, parts[0])
                    };
                    Some((host_path, parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();

        ServiceTemplate {
            image: config.image,
            ports,
            environment,
            volumes,
            compatibility,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_offering(dir: &Path, category: &str, name: &str) {
        let cat_dir = dir.join(category);
        fs::create_dir_all(&cat_dir).unwrap();

        // Create snippet
        fs::write(
            cat_dir.join(format!("{}.snippet.yaml", name)),
            format!("image: {}:latest\nports:\n  - [8080, 8080]", name),
        ).unwrap();

        // Create frontmatter
        fs::write(
            cat_dir.join(format!("{}.frontmatter.json", name)),
            format!(r#"{{"description": "Test {} service", "tags": ["test"]}}"#, name),
        ).unwrap();
    }

    #[test]
    fn test_load_empty_directory() {
        let temp = TempDir::new().unwrap();
        let result = SwManifests::load(temp.path());
        assert!(result.is_ok());
        let manifests = result.unwrap();
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_load_nonexistent_directory() {
        let result = SwManifests::load(Path::new("/nonexistent/path/12345"));
        assert!(result.is_ok());
        let manifests = result.unwrap();
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_load_offerings() {
        let temp = TempDir::new().unwrap();
        create_test_offering(temp.path(), "data", "mongodb");
        create_test_offering(temp.path(), "cache", "redis");

        let manifests = SwManifests::load(temp.path()).unwrap();

        assert_eq!(manifests.len(), 2);
        assert!(manifests.contains("mongodb"));
        assert!(manifests.contains("redis"));
        assert_eq!(manifests.categories.len(), 2);
        assert!(manifests.categories.contains(&"data".to_string()));
        assert!(manifests.categories.contains(&"cache".to_string()));
    }

    #[test]
    fn test_get_by_category() {
        let temp = TempDir::new().unwrap();
        create_test_offering(temp.path(), "data", "mongodb");
        create_test_offering(temp.path(), "data", "postgresql");
        create_test_offering(temp.path(), "cache", "redis");

        let manifests = SwManifests::load(temp.path()).unwrap();

        let data_offerings = manifests.by_category("data");
        assert_eq!(data_offerings.len(), 2);

        let cache_offerings = manifests.by_category("cache");
        assert_eq!(cache_offerings.len(), 1);
    }

    #[test]
    fn test_entry_description() {
        let entry = SwEntry {
            name: "test".to_string(),
            category: "data".to_string(),
            snippet_yaml: "image: test".to_string(),
            compatibility: None,
            frontmatter: Some(SwFrontmatter {
                description: Some("My test service".to_string()),
                category: None,
                tags: None,
                icon: None,
                homepage: None,
                documentation: None,
            }),
        };

        assert_eq!(entry.description(), "My test service");

        let entry_no_fm = SwEntry {
            name: "test".to_string(),
            category: "data".to_string(),
            snippet_yaml: "image: test".to_string(),
            compatibility: None,
            frontmatter: None,
        };

        assert_eq!(entry_no_fm.description(), "test service");
    }
}
