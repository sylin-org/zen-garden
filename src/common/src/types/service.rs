//! Service types — operational state of running offerings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::hardware::ContainerResources;
use super::offering::OfferingGuidance;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    /// Service is being installed (image pull, container creation)
    Installing,
    Running,
    Stopped,
    Maintenance,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Unique identifier for this offering instance (GUIDv7)
    /// Survives renames, migrations, and is used for backup keying.
    /// Pure GUIDv7 format (e.g., "018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f")
    #[serde(default)]
    pub offering_id: String,
    pub name: String,
    pub offering: String,
    pub version: String,
    pub status: ServiceStatus,
    pub health: super::health::ServiceHealthStatus,
    pub ports: Ports,
    pub resources: Option<ContainerResources>,
    /// Job ID for tracking installation progress (only set when status is Installing)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub job_id: Option<String>,
    /// Sub-capabilities discovered at runtime (e.g., models, plugins)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sub_capabilities: Vec<SubCapability>,
    /// Cached post-installation guidance (templated at install time)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub guidance: Option<OfferingGuidance>,
    /// Owners who have applied config patches to this service.
    /// e.g., ["mongodb-orchestrator"]. Empty = vanilla config.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub customized_by: Vec<String>,
}

// ── Sub-Capability Types (runtime-discovered features) ──────────────

/// Sub-capability of a service discovered at runtime
///
/// Examples:
/// - ollama: models (llama2, mistral, neural-chat)
/// - milvus: collections (embeddings, documents)
/// - plugins: extensions (auth, metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubCapability {
    /// Capability type (e.g., "model", "collection", "plugin")
    #[serde(rename = "type")]
    pub cap_type: String,
    /// List of capability names/identifiers
    pub items: Vec<String>,
    /// When these capabilities were last discovered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SubCapability {
    /// Create a new sub-capability with current timestamp
    pub fn new(cap_type: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            cap_type: cap_type.into(),
            items,
            discovered_at: Some(chrono::Utc::now()),
        }
    }

    /// Create from a CapabilityCollection (extracts just names)
    pub fn from_collection(collection: &CapabilityCollection) -> Self {
        Self {
            cap_type: collection.cap_type.clone(),
            items: collection.items.iter().map(|i| i.name.clone()).collect(),
            discovered_at: Some(chrono::Utc::now()),
        }
    }

    /// Check if this capability includes a specific item
    pub fn has(&self, item: &str) -> bool {
        self.items
            .iter()
            .any(|i| i == item || i.to_lowercase() == item.to_lowercase())
    }

    /// Get the count of items
    pub fn count(&self) -> usize {
        self.items.len()
    }
}

/// Rich capability item with metadata (used in capability API responses)
///
/// This is the normalized format for capability items across all offerings.
/// Commands output JSON that maps to this structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityItem {
    /// Capability name (required) - the identifier
    pub name: String,

    /// Optional version string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional human-readable size (e.g., "4.2 GB")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Optional size in bytes (for sorting/comparison)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,

    /// Optional status (e.g., "active", "loaded", "enabled")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Arbitrary metadata (offering-specific details)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CapabilityItem {
    /// Create a new capability item with just a name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Builder: set size in bytes (auto-computes human-readable size)
    pub fn with_size_bytes(mut self, bytes: u64) -> Self {
        self.size_bytes = Some(bytes);
        self.size = Some(format_bytes(bytes));
        self
    }

    /// Builder: set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Builder: add metadata field
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Display configuration for capability type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDisplay {
    /// Singular form (e.g., "model")
    pub singular: String,
    /// Plural form (e.g., "models")
    pub plural: String,
}

impl Default for CapabilityDisplay {
    fn default() -> Self {
        Self {
            singular: "capability".to_string(),
            plural: "capabilities".to_string(),
        }
    }
}

/// Collection of capabilities of a single type (used in API responses)
///
/// Represents all capabilities of one type (e.g., all models for Ollama).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityCollection {
    /// Capability type (e.g., "model", "extension", "module")
    #[serde(rename = "type")]
    pub cap_type: String,

    /// Display labels
    pub display: CapabilityDisplay,

    /// The capability items
    pub items: Vec<CapabilityItem>,

    /// When these capabilities were discovered
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}

impl CapabilityCollection {
    /// Create a new collection
    pub fn new(cap_type: impl Into<String>, items: Vec<CapabilityItem>) -> Self {
        Self {
            cap_type: cap_type.into(),
            display: CapabilityDisplay::default(),
            items,
            discovered_at: chrono::Utc::now(),
        }
    }

    /// Builder: set display labels
    pub fn with_display(mut self, singular: impl Into<String>, plural: impl Into<String>) -> Self {
        self.display = CapabilityDisplay {
            singular: singular.into(),
            plural: plural.into(),
        };
        self
    }

    /// Get count of items
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Check if collection contains an item by name (case-insensitive)
    pub fn has(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.items.iter().any(|i| i.name.to_lowercase() == lower)
    }

    /// Convert to lightweight SubCapability (for ServiceInfo storage)
    pub fn to_sub_capability(&self) -> SubCapability {
        SubCapability::from_collection(self)
    }

    /// Get summary string for rake list (e.g., "4 models")
    pub fn summary(&self) -> String {
        let count = self.count();
        if count == 1 {
            format!("1 {}", self.display.singular)
        } else {
            format!("{} {}", count, self.display.plural)
        }
    }
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ports {
    pub native: u16,
    pub agnostic: Option<u16>,
}
