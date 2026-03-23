//! Capability manifest schema
//!
//! Defines how offerings discover, add, and remove their sub-capabilities
//! (e.g., Ollama models, PostgreSQL extensions, Redis modules).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root structure for a capability manifest file
///
/// File pattern: `{offering}.capabilities.yaml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// Schema version
    #[serde(default = "default_version")]
    pub version: String,

    /// Parent offering name (must match an existing offering)
    pub offering: String,

    /// Capability type definitions
    pub capabilities: Vec<CapabilityTypeConfig>,
}

fn default_version() -> String {
    "1".to_string()
}

/// Configuration for a single capability type (e.g., "model" for Ollama)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityTypeConfig {
    /// Capability type identifier (e.g., "model", "extension", "module")
    #[serde(rename = "type")]
    pub cap_type: String,

    /// Display configuration
    pub display: CapabilityDisplayConfig,

    /// Mutability mode
    #[serde(default)]
    pub mutability: MutabilityMode,

    /// List operation configuration
    pub list: ListOperationConfig,

    /// Add operation configuration (optional - may not be supported)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add: Option<AddOperationConfig>,

    /// Remove operation configuration (optional - may not be supported)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<RemoveOperationConfig>,

    /// Check for updates operation (optional - detects if updates are available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_updates: Option<CheckUpdatesConfig>,

    /// Upgrade operation (optional - upgrades existing capability to latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<UpgradeOperationConfig>,

    /// Summary configuration for rake list display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryConfig>,
}

/// Display configuration for capability type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDisplayConfig {
    /// Singular form (e.g., "model")
    pub singular: String,
    /// Plural form (e.g., "models")
    pub plural: String,
}

impl Default for CapabilityDisplayConfig {
    fn default() -> Self {
        Self {
            singular: "capability".to_string(),
            plural: "capabilities".to_string(),
        }
    }
}

/// Mutability mode - determines if capabilities can be changed at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MutabilityMode {
    /// Changes take effect immediately (default)
    #[default]
    Hot,
    /// Changes require service restart
    Warm,
    /// Changes require container rebuild
    Cold,
}

/// Commands configuration - supports both managed (container) and adopted (native) modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeCommands {
    /// Commands for managed (containerized) offerings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<PlatformCommands>,

    /// Commands for adopted (native) offerings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopted: Option<PlatformCommands>,
}

/// Platform-specific commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCommands {
    /// Linux command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<String>,

    /// Windows command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,

    /// macOS command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos: Option<String>,
}

impl PlatformCommands {
    /// Get command for current platform
    pub fn for_current_platform(&self) -> Option<&str> {
        #[cfg(target_os = "linux")]
        {
            self.linux.as_deref()
        }
        #[cfg(target_os = "windows")]
        {
            self.windows.as_deref()
        }
        #[cfg(target_os = "macos")]
        {
            self.macos.as_deref()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            None
        }
    }
}

/// Transform specification for normalizing command output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformSpec {
    /// JSONPath to the items array (e.g., ".models")
    pub items_path: String,

    /// Field mappings from source to CapabilityItem fields
    pub fields: FieldMappings,
}

/// Field mappings for transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappings {
    /// Path to name field (required)
    pub name: String,

    /// Path to version field (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Path to size_bytes field (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<String>,

    /// Metadata field mappings (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// Configuration for the LIST operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOperationConfig {
    /// Commands to fetch raw data (mode and platform specific)
    pub commands: ModeCommands,

    /// Transform specification for normalizing output
    pub transform: TransformSpec,

    /// Expected output format
    #[serde(default)]
    pub output: OutputFormat,

    /// Timeout in seconds
    #[serde(default = "default_list_timeout")]
    pub timeout_secs: u64,
}

fn default_list_timeout() -> u64 {
    10
}

/// Configuration for the ADD operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOperationConfig {
    /// Whether this operation is available
    #[serde(default = "default_true")]
    pub available: bool,

    /// Reason if not available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Commands to add capability (mode and platform specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<ModeCommands>,

    /// Timeout in seconds
    #[serde(default = "default_add_timeout")]
    pub timeout_secs: u64,

    /// Progress extraction pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressConfig>,
}

fn default_add_timeout() -> u64 {
    7200 // 2 hours for large model downloads
}

fn default_true() -> bool {
    true
}

/// Configuration for the REMOVE operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveOperationConfig {
    /// Whether this operation is available
    #[serde(default = "default_true")]
    pub available: bool,

    /// Reason if not available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Commands to remove capability (mode and platform specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<ModeCommands>,

    /// Timeout in seconds
    #[serde(default = "default_remove_timeout")]
    pub timeout_secs: u64,
}

fn default_remove_timeout() -> u64 {
    60
}

/// Configuration for the CHECK_UPDATES operation
///
/// Detects if a capability has an update available by comparing local vs remote state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckUpdatesConfig {
    /// Whether this operation is available
    #[serde(default = "default_true")]
    pub available: bool,

    /// Command to get local capability info (returns JSON with digest/version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_command: Option<ModeCommands>,

    /// Command to get remote/registry info (returns JSON with digest/version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_command: Option<ModeCommands>,

    /// Comparison specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<UpdateCompareSpec>,

    /// Timeout in seconds
    #[serde(default = "default_check_timeout")]
    pub timeout_secs: u64,
}

fn default_check_timeout() -> u64 {
    30
}

/// Specification for comparing local vs remote capability state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCompareSpec {
    /// JSONPath to local version/digest
    pub local_path: String,

    /// JSONPath to remote version/digest
    pub remote_path: String,
}

/// Configuration for the UPGRADE operation
///
/// Upgrades an existing capability to the latest version.
/// Semantically distinct from ADD (which installs new capabilities).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeOperationConfig {
    /// Whether this operation is available
    #[serde(default = "default_true")]
    pub available: bool,

    /// Reason if not available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Commands to upgrade capability (mode and platform specific)
    /// If not specified, falls back to add commands
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<ModeCommands>,

    /// Timeout in seconds
    #[serde(default = "default_upgrade_timeout")]
    pub timeout_secs: u64,

    /// Progress extraction pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressConfig>,
}

fn default_upgrade_timeout() -> u64 {
    7200 // Same as add - 2 hours for large downloads
}

/// Progress extraction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressConfig {
    /// Regex pattern to extract progress percentage
    pub pattern: String,
}

/// Summary configuration for rake list display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryConfig {
    /// Transform to get count (optional - defaults to items.len())
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<SummaryTransform>,

    /// Format string (e.g., "{{count}} models")
    pub format: String,
}

/// Summary transform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryTransform {
    /// JSONPath to count value
    pub count_path: String,
}

/// Output format expectation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// JSON output (parsed as JSON)
    #[default]
    Json,
    /// Line-based output (each line is an item name)
    Lines,
    /// Single number (for summary/count)
    Number,
}

impl CapabilityManifest {
    /// Parse a capability manifest from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yml::Error> {
        serde_yml::from_str(yaml)
    }

    /// Get capability type config by type name
    pub fn get_capability_type(&self, cap_type: &str) -> Option<&CapabilityTypeConfig> {
        self.capabilities.iter().find(|c| c.cap_type == cap_type)
    }
}

impl CapabilityTypeConfig {
    /// Check if add operation is available
    pub fn can_add(&self) -> bool {
        self.add.as_ref().map(|a| a.available).unwrap_or(false)
    }

    /// Check if remove operation is available
    pub fn can_remove(&self) -> bool {
        self.remove.as_ref().map(|r| r.available).unwrap_or(false)
    }

    /// Check if check_updates operation is available
    pub fn can_check_updates(&self) -> bool {
        self.check_updates
            .as_ref()
            .map(|c| c.available)
            .unwrap_or(false)
    }

    /// Check if upgrade operation is available
    pub fn can_upgrade(&self) -> bool {
        self.upgrade.as_ref().map(|u| u.available).unwrap_or(false)
    }

    /// Check if capabilities can be modified at runtime
    pub fn is_mutable(&self) -> bool {
        self.mutability == MutabilityMode::Hot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ollama_manifest() {
        let yaml = r#"
version: "1"
offering: ollama

capabilities:
  - type: model
    display:
      singular: model
      plural: models
    mutability: hot

    list:
      commands:
        managed:
          linux: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
        adopted:
          linux: "curl -s http://localhost:{{port}}/api/tags"
      transform:
        items_path: ".models"
        fields:
          name: ".name"
          size_bytes: ".size"
          metadata:
            family: ".details.family"
      output: json
      timeout_secs: 10

    add:
      available: true
      commands:
        adopted:
          linux: "ollama pull {{item}}"
      timeout_secs: 7200
      progress:
        pattern: "(\\d+)%"

    remove:
      available: true
      commands:
        adopted:
          linux: "ollama rm {{item}}"
      timeout_secs: 60

    summary:
      format: "{{count}} models"
"#;

        let manifest = CapabilityManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.offering, "ollama");
        assert_eq!(manifest.capabilities.len(), 1);

        let cap = &manifest.capabilities[0];
        assert_eq!(cap.cap_type, "model");
        assert_eq!(cap.display.singular, "model");
        assert_eq!(cap.mutability, MutabilityMode::Hot);
        assert!(cap.can_add());
        assert!(cap.can_remove());
        assert!(cap.is_mutable());
    }

    #[test]
    fn test_parse_cold_mutability() {
        let yaml = r#"
version: "1"
offering: redis

capabilities:
  - type: module
    display:
      singular: module
      plural: modules
    mutability: cold

    list:
      commands:
        managed:
          linux: "docker exec {{container_name}} redis-cli MODULE LIST"
      transform:
        items_path: "."
        fields:
          name: ".name"
      output: json
      timeout_secs: 10

    add:
      available: false
      reason: "Redis modules require container rebuild"

    remove:
      available: false
      reason: "Redis modules cannot be unloaded at runtime"
"#;

        let manifest = CapabilityManifest::from_yaml(yaml).unwrap();
        let cap = &manifest.capabilities[0];

        assert_eq!(cap.mutability, MutabilityMode::Cold);
        assert!(!cap.can_add());
        assert!(!cap.can_remove());
        assert!(!cap.is_mutable());

        assert_eq!(
            cap.add.as_ref().unwrap().reason.as_deref(),
            Some("Redis modules require container rebuild")
        );
    }
}
