//! Zen Common Types
//! Core data structures for service discovery, health, resources, and registry

use crate::constants::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod peer_address;
pub mod topology;

// ============================================================================
// Service Types
// ============================================================================

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
    pub health: ServiceHealthStatus,
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
}

// ============================================================================
// Sub-Capability Types (runtime-discovered features)
// ============================================================================

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

// ============================================================================
// Health Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceHealthStatus {
    Healthy,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: String, // "pass", "warn", or "fail"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String, // "healthy", "degraded", or "unhealthy"
    #[serde(flatten)]
    pub details: HashMap<String, serde_json::Value>,
}

impl ComponentHealth {
    pub fn healthy(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_HEALTHY.to_string(),
            details,
        }
    }

    pub fn degraded(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_DEGRADED.to_string(),
            details,
        }
    }

    pub fn unhealthy(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_UNHEALTHY.to_string(),
            details,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealthStatus {
    pub status: String,    // "healthy", "degraded", or "unhealthy"
    pub version: String,   // Software version (e.g., "0.1.202601231053")
    pub timestamp: String, // ISO 8601 timestamp
    pub components: HashMap<String, ComponentHealth>,
    // Platform information for deployment tools
    pub os: String,           // Operating system (e.g., "windows", "linux", "macos")
    pub architecture: String, // CPU architecture (e.g., "x86_64", "aarch64")
    // Legacy fields for backward compatibility
    #[serde(skip_serializing)]
    pub docker_available: bool,
    #[serde(skip_serializing)]
    pub disk_space_ok: bool,
    #[serde(skip_serializing)]
    pub memory_ok: bool,
    #[serde(skip_serializing)]
    pub uptime_seconds: u64,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub checks: HashMap<String, HealthCheck>,
}

// ============================================================================
// Resource Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiskType {
    NVMe,
    SSD,
    HDD,
    Unknown,
}

/// Storage metrics (live data, collected every 30s)
///
/// Replaces separate static inventory + dynamic usage approach.
/// Contains both device info and current usage in one structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub identifier: String,  // e.g., "sda", "nvme0n1", "C:"
    pub mount_point: String, // e.g., "/", "/data", "C:\"
    pub total_gb: u64,
    pub used_gb: u64,
    pub available_gb: u64,
    pub used_percent: f32,
    pub disk_type: DiskType,
    pub filesystem: String, // e.g., "ext4", "NTFS"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiRuntime {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cuda_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rocm_version: Option<String>,
    #[serde(default)]
    pub has_directml: bool,
    #[serde(default)]
    pub has_openvino: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>, // "cuda", "rocm", "vulkan", "directml", "opencl"

    /// Detected AI runtimes in dual format
    /// Supports both simple ("cuda") and versioned ("cuda:12.2") formats
    /// Example: ["cuda", "cuda:12.2", "directml"]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ai_runtimes: Vec<String>,
}

/// Live system resources (collected every 5s for CPU/memory, 30s for storage)
///
/// This is the single source of truth for runtime metrics.
/// Storage inventory included here due to semi-dynamic nature (hot-swap, mounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneResources {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub storage: Vec<StorageMetrics>, // All mounted disks with live usage
    pub uptime_seconds: u64,
    pub uptime_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    pub cores: usize,
    pub usage_percent: f32,
    pub usage_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f32,
    pub total_friendly: String,
    pub used_friendly: String,
    pub available_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f32,
    pub path: String,
    pub total_friendly: String,
    pub used_friendly: String,
    pub available_friendly: String,
}

/// Hardware capability detection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectionStatus {
    /// Detection not yet started or in early stages
    Scanning,
    /// CPU and memory detected, GPU detection in progress
    Partial,
    /// All hardware detection complete
    Complete,
}

/// AI capabilities summary aggregated across all GPUs
///
/// This provides a quick overview of available AI acceleration without
/// needing to iterate through individual GPUs. Useful for:
/// - Fast capability checks ("has any AI runtime?")
/// - Service placement decisions
/// - Lantern service discovery
/// - Health monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCapabilitiesSummary {
    /// All available runtimes (deduplicated across GPUs)
    /// Supports both simple format ("cuda") and versioned format ("cuda:12.2")
    pub runtimes: Vec<String>,

    /// All GPU vendors present (lowercase)
    pub vendors: Vec<String>,

    /// Total VRAM across all GPUs (MB)
    pub total_vram_mb: u64,

    /// Number of AI-capable GPUs
    pub gpu_count: usize,

    /// Whether hardware detection is complete
    pub detection_complete: bool,
}

impl AiCapabilitiesSummary {
    /// Check if any AI acceleration is available
    pub fn has_any_acceleration(&self) -> bool {
        !self.runtimes.is_empty()
    }

    /// Check if a specific runtime is available (case-insensitive)
    /// Supports both "cuda" and "cuda:12.2" format checks
    pub fn supports_runtime(&self, runtime: &str) -> bool {
        let runtime_lower = runtime.to_lowercase();
        self.runtimes.iter().any(|r| {
            let r_lower = r.to_lowercase();
            // Match either exact or base runtime (e.g., "cuda" matches "cuda:12.2")
            r_lower == runtime_lower || r_lower.starts_with(&format!("{}:", runtime_lower))
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareCapabilities {
    /// Unique stone identifier (GUID v7, generated once on first boot)
    /// Immutable even if hostname changes. Used for cache keying and distributed tracking.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub hardware: HardwareInventory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeInfo>,
    pub detection_status: DetectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInventory {
    pub cpu: CpuCapabilities,
    pub memory: MemoryCapabilities,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub gpus: Vec<GpuInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub swap_mb: Option<u64>,

    /// AI capabilities summary (NEW - backwards compatible)
    /// Aggregated view of AI acceleration across all GPUs
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_capabilities: Option<AiCapabilitiesSummary>,

    /// System manufacturer from DMI/SMBIOS (e.g., "Dell Inc.")
    /// Used for hardware manifest matching
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system_manufacturer: Option<String>,

    /// System product name from DMI/SMBIOS (e.g., "Wyse 5070")
    /// Used for hardware manifest matching
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system_product: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cores: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<usize>,
    pub architecture: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub features: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCapabilities {
    pub total_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskCapabilities {
    pub total_gb: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_type: Option<String>, // "SSD", "HDD", "NVMe"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_version: Option<String>,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: String,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkMetrics>,
    pub uptime_seconds: u64,
}

/// Network metrics for all interfaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Per-interface statistics
    pub interfaces: Vec<InterfaceMetrics>,
    /// Total bytes received across all interfaces
    pub total_rx_bytes: u64,
    /// Total bytes transmitted across all interfaces
    pub total_tx_bytes: u64,
    /// Aggregate receive rate (bytes/sec) - requires sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_bytes_per_sec: Option<u64>,
    /// Aggregate transmit rate (bytes/sec) - requires sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_bytes_per_sec: Option<u64>,
    /// Friendly display strings
    pub total_rx_friendly: String,
    pub total_tx_friendly: String,
}

/// Per-interface network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceMetrics {
    /// Interface name (e.g., "eth0", "wlan0", "Ethernet")
    pub name: String,
    /// Total bytes received since boot
    pub rx_bytes: u64,
    /// Total bytes transmitted since boot
    pub tx_bytes: u64,
    /// Human-readable received bytes
    pub rx_friendly: String,
    /// Human-readable transmitted bytes
    pub tx_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResources {
    pub cpu_percent: f32,
    pub cpu_friendly: String,
    pub memory_bytes: u64,
    pub memory_limit_bytes: u64,
    pub memory_percent: f32,
    pub memory_friendly: String,
    pub memory_limit_friendly: String,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub network_rx_friendly: String,
    pub network_tx_friendly: String,
    pub block_read_bytes: u64,
    pub block_write_bytes: u64,
    pub block_read_friendly: String,
    pub block_write_friendly: String,
    pub uptime_seconds: u64,
    pub uptime_friendly: String,
}

// ============================================================================
// Discovery Protocol Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    pub discover: String,
    pub request_id: String,
    pub requester: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub stone_name: String,
    /// Network address of the responding stone.
    pub address: crate::PeerAddress,
    pub moss_version: String,
    pub lantern_endpoint: Option<String>,
}

// ============================================================================
// UDP Announcement Envelope (unified message format)
// ============================================================================

/// UDP Announcement envelope for type-safe message routing
///
/// All UDP broadcasts use this envelope format. Consumers filter by `announcement_type`
/// and deserialize `data` into the appropriate typed payload.
///
/// # Example
/// ```ignore
/// let announcement = UdpAnnouncement {
///     msg_id: Some(generate_guidv7()),
///     announcement_type: announcement_types::STONE_CHIRP.to_string(),
///     data: serde_json::to_value(&chirp_payload)?,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpAnnouncement {
    /// Optional message ID for deduplication (GUIDv7)
    /// When present, receivers will deduplicate messages with same ID within 5s window.
    /// This handles multi-path delivery (multicast + broadcast arriving separately).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    /// Announcement type discriminator
    #[serde(rename = "type")]
    pub announcement_type: String,
    /// Typed payload (deserialize based on announcement_type)
    pub data: serde_json::Value,
    /// Base64-encoded ECDSA signature over the serialized `data` field.
    /// Present when the sender is enrolled in a pond (Phase 2 signed chirps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// PEM-encoded sender public key (SPKI format, `BEGIN PUBLIC KEY`).
    /// Phase 2: bare public key for direct signature verification.
    /// Phase 4: will add full cert for CA chain validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_cert: Option<String>,
}

/// Service information for topology entries and chirp payloads
///
/// Lightweight representation of service state for UDP topology broadcasts.
/// Full ServiceInfo (with health, ports, resources) is used in local registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyServiceEntry {
    /// Unique identifier for this offering instance (GUIDv7)
    /// Survives renames, migrations, used for backup keying.
    #[serde(default)]
    pub offering_id: String,
    pub name: String,
    pub offering: String,
    pub category: String,
    pub status: String,
}

impl TopologyServiceEntry {
    /// Convert full ServiceInfo to lightweight TopologyServiceEntry
    /// Used when syncing registry to self_entry for chirp broadcasts
    pub fn from_service_info(service: &ServiceInfo, category: Option<&str>) -> Self {
        Self {
            offering_id: service.offering_id.clone(),
            name: service.name.clone(),
            offering: service.offering.clone(),
            category: category.unwrap_or(&service.offering).to_string(),
            status: match service.status {
                ServiceStatus::Running => SERVICE_RUNNING,
                ServiceStatus::Stopped => SERVICE_STOPPED,
                ServiceStatus::Installing => SERVICE_INSTALLING,
                ServiceStatus::Maintenance => SERVICE_MAINTENANCE,
                ServiceStatus::Degraded => SERVICE_DEGRADED,
                ServiceStatus::Unknown => SERVICE_UNKNOWN,
            }
            .to_string(),
        }
    }

    /// Batch convert ServiceInfo vec to TopologyServiceEntry vec
    pub fn from_service_infos(services: &[ServiceInfo]) -> Vec<Self> {
        services
            .iter()
            .map(|svc| Self::from_service_info(svc, None))
            .collect()
    }

    /// Create from Offering
    pub fn from_offering(offering: &Offering) -> Self {
        Self {
            offering_id: offering.offering_id.clone(),
            name: offering.name.clone(),
            offering: offering.offering.clone(),
            category: offering.offering.clone(), // Use offering as category
            status: match offering.status {
                OfferingStatus::Running => SERVICE_RUNNING,
                OfferingStatus::Stopped => SERVICE_STOPPED,
                OfferingStatus::Installing => SERVICE_INSTALLING,
                OfferingStatus::Maintenance => SERVICE_MAINTENANCE,
                OfferingStatus::Degraded => SERVICE_DEGRADED,
                OfferingStatus::Unknown => SERVICE_UNKNOWN,
            }
            .to_string(),
        }
    }

    /// Batch convert Offering vec to TopologyServiceEntry vec
    pub fn from_offerings(offerings: &[Offering]) -> Vec<Self> {
        offerings.iter().map(Self::from_offering).collect()
    }
}

/// Stone connectivity status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoneStatus {
    /// Stone is actively announcing (seen within threshold)
    Online,
    /// Stone has stopped announcing but is remembered for WoL
    Offline,
}

impl std::fmt::Display for StoneStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoneStatus::Online => write!(f, "online"),
            StoneStatus::Offline => write!(f, "offline"),
        }
    }
}

// TopologyEntry is defined in types::topology and re-exported from lib.rs.
// Do not duplicate it here — see types/topology.rs for the canonical definition.

/// Stone goodbye payload - sent when stone is shutting down gracefully
///
/// Enables immediate offline marking instead of waiting for chirp timeout.
/// Minimal payload - just identification fields needed to find the stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneGoodbyePayload {
    pub stone_id: String,
    pub stone_name: String,
}

// ============================================================================
// Lantern Service Registry Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub endpoint: String,
    pub services: Vec<RegisterServiceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterServiceInfo {
    pub name: String,
    pub service_type: String,
    pub status: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub ttl_seconds: u32,
    pub next_heartbeat_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub service_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveResponse {
    pub stone_name: String,
    pub endpoint: String,
    pub service: ResolveServiceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveServiceInfo {
    pub name: String,
    pub service_type: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternTopology {
    pub stones: Vec<LanternStoneState>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternStoneState {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub name: String,
    pub endpoint: String,
    pub status: String,
    pub services: Vec<LanternServiceState>,
    pub last_seen: String,
    pub first_seen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline_since: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternServiceState {
    pub name: String,
    pub service_type: String,
    pub status: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenEvent {
    pub event_type: String,
    pub timestamp: String,
    pub stone_name: String,
    pub details: serde_json::Value,
}

// ============================================================================
// Pond Security Types (Phase 1: surface defined, no implementation)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PondConfig {
    pub enabled: bool,
    pub keystone_path: Option<String>,
    pub require_mtls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystoneRequest {
    pub pond_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInviteRequest {
    pub stone_name: String,
    pub expiry_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInviteResponse {
    pub invitation_code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceStoneRequest {
    pub invitation_code: String,
}

// ============================================================================
// Compatibility System Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityRules {
    pub version: String,
    pub compatibility_rules: Vec<CompatibilityRule>,
    pub post_install_healthcheck: Option<PostInstallHealthcheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityRule {
    pub name: String,
    pub condition: RuleCondition,
    pub reason: String,
    pub suggestion: Option<String>,
    pub fallback: Option<FallbackConfig>,
    /// If true, this rule produces a warning instead of failing installation.
    /// Use for "proceed with caution" scenarios where the offering may work
    /// but has known issues on certain hardware.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warn_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    pub processor_models: Option<Vec<String>>,
    pub processor_patterns: Option<Vec<String>>,
    pub cpu_features_missing: Option<Vec<String>>,
    pub architectures: Option<Vec<String>>,
    pub memory_mb_less_than: Option<u64>,

    // OS/Platform requirements
    /// Match if OS family is in this list (e.g., ["linux", "macos"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_family: Option<Vec<String>>,
    /// Match if OS family is NOT in this list (e.g., ["windows"] to block Windows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_family_not: Option<Vec<String>>,

    // AI/GPU requirements
    /// Match if ANY of the listed AI runtimes are present (OR logic: ['cuda', 'rocm'])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_ai_any: Option<Vec<String>>,
    /// Match if ALL of the listed AI runtimes are present (AND logic: ['cuda', 'directml'])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_ai_all: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb_less_than: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb_at_least: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostInstallHealthcheck {
    pub enabled: bool,
    pub scan_log_lines: usize,
    pub timeout_seconds: u64,
    pub patterns: Vec<HealthcheckPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckPattern {
    pub pattern: String,
    pub reason: String,
    pub suggestion: Option<String>,
    pub fallback: Option<FallbackConfig>,
}

// ============================================================================
// Well-Known Ports Catalog Types
// ============================================================================

/// Catalog of well-known ports with conflict detection and remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellKnownPortsCatalog {
    pub version: String,
    pub ports: std::collections::HashMap<u16, WellKnownPort>,
}

/// Definition of a well-known port with platform-specific conflict handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellKnownPort {
    pub name: String,
    pub description: String,
    /// Default remediation for all platforms (used if no platform-specific handler)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<PortRemediation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<PortConflictHandler>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<PortConflictHandler>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<PortConflictHandler>,
}

/// Platform-specific conflict detection and remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConflictHandler {
    /// Common service that uses this port
    pub common_culprit: String,
    /// Command to detect if the culprit is active (exit 0 = active)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection: Option<String>,
    /// Remediation strategy
    pub remediation: PortRemediation,
}

/// Remediation strategy for port conflicts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PortRemediation {
    /// Remap to next available port in range (default for most services)
    Remap {
        /// Start of port range to search
        range_start: u16,
        /// End of port range to search
        range_end: u16,
    },
    /// Automatically run commands to free the port (for essential ports like DNS)
    Auto {
        /// Commands to run to free the port
        commands: Vec<String>,
        /// Files to create after remediation
        #[serde(default, skip_serializing_if = "Option::is_none")]
        files: Option<Vec<RemediationFile>>,
    },
    /// Show message and fail - user must manually resolve
    Manual { message: String },
    /// Fail with error - no remediation possible
    Fail { message: String },
}

/// File to create as part of remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationFile {
    pub path: String,
    pub content: String,
}

// ============================================================================
// Offering Modes Types (Multi-deployment patterns)
// ============================================================================

/// Deployment mode for an offering
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfferingMode {
    /// Container-based offering managed by Moss (default, current system)
    Managed,
    /// Existing service (native or containerized) adopted by Moss
    Adopted,
    /// External network service announced by Moss
    Borrowed,
}

/// Control level for adopted offerings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AdoptedControlLevel {
    /// Moss manages lifecycle (start/stop/restart)
    Full,
    /// Moss monitors health only (default - safe)
    #[default]
    Monitor,
    /// Moss announces existence only (discovery)
    Announce,
}

/// Health check method for borrowed offerings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthMethod {
    /// HTTP endpoint probe
    Http,
    /// TCP socket connectivity
    Tcp,
    /// No health check (always assume healthy)
    None,
}

// ============================================================================
// Unified Offering Types (Runtime Instances)
// ============================================================================

/// Offering instance representing any running/adopted/borrowed service
///
/// A unified structure that uses an enum for mode-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offering {
    // ═══════════════════════════════════════════════════════════════
    // IDENTITY (common to all modes)
    // ═══════════════════════════════════════════════════════════════
    /// Unique identifier (GUIDv7) - generated for all modes
    pub offering_id: String,

    /// Instance name (e.g., "my-mongodb", "ollama:adopted")
    pub name: String,

    /// Offering type/template name (e.g., "mongodb", "ollama")
    pub offering: String,

    /// Version string (always present, "unknown" if undetected)
    #[serde(default = "default_version_unknown")]
    pub version: String,

    // ═══════════════════════════════════════════════════════════════
    // STATE (common to all modes)
    // ═══════════════════════════════════════════════════════════════
    /// Current operational status
    pub status: OfferingStatus,

    /// Health status (unified across modes)
    pub health: ServiceHealthStatus,

    /// Runtime-discovered capabilities (models, extensions, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_capabilities: Vec<SubCapability>,

    // ═══════════════════════════════════════════════════════════════
    // LOCATION (unified port handling)
    // ═══════════════════════════════════════════════════════════════
    /// Service network location
    pub location: OfferingLocation,

    // ═══════════════════════════════════════════════════════════════
    // MODE-SPECIFIC DATA (enum with associated data)
    // ═══════════════════════════════════════════════════════════════
    /// Mode-specific configuration and state
    pub mode_data: OfferingModeData,

    // ═══════════════════════════════════════════════════════════════
    // TIMESTAMPS
    // ═══════════════════════════════════════════════════════════════
    /// When this offering was first registered/detected/announced
    pub registered_at: chrono::DateTime<chrono::Utc>,

    /// When this offering was last updated (status change, health change, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_version_unknown() -> String {
    "unknown".to_string()
}

/// Unified offering status (expanded from ServiceStatus)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfferingStatus {
    /// Being installed/pulled (managed only)
    Installing,
    /// Running and operational
    Running,
    /// Stopped or not running
    Stopped,
    /// In maintenance mode
    Maintenance,
    /// Running but degraded
    Degraded,
    /// Status cannot be determined
    Unknown,
}

impl std::fmt::Display for OfferingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installing => write!(f, "installing"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Maintenance => write!(f, "maintenance"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl From<ServiceStatus> for OfferingStatus {
    fn from(status: ServiceStatus) -> Self {
        match status {
            ServiceStatus::Installing => Self::Installing,
            ServiceStatus::Running => Self::Running,
            ServiceStatus::Stopped => Self::Stopped,
            ServiceStatus::Maintenance => Self::Maintenance,
            ServiceStatus::Degraded => Self::Degraded,
            ServiceStatus::Unknown => Self::Unknown,
        }
    }
}

impl From<OfferingStatus> for ServiceStatus {
    fn from(status: OfferingStatus) -> Self {
        match status {
            OfferingStatus::Installing => Self::Installing,
            OfferingStatus::Running => Self::Running,
            OfferingStatus::Stopped => Self::Stopped,
            OfferingStatus::Maintenance => Self::Maintenance,
            OfferingStatus::Degraded => Self::Degraded,
            OfferingStatus::Unknown => Self::Unknown,
        }
    }
}

/// Unified network location (replaces Ports + ServiceLocation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingLocation {
    /// Host address (localhost for managed, configurable for adopted/borrowed)
    #[serde(default = "default_localhost")]
    pub host: String,

    /// Primary service port
    pub port: u16,

    /// Protocol hint (http, tcp, mongodb, postgres, etc.)
    #[serde(default = "default_protocol")]
    pub protocol: String,

    /// Optional agnostic port (managed containers only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agnostic_port: Option<u16>,
}

fn default_localhost() -> String {
    "localhost".to_string()
}

fn default_protocol() -> String {
    "http".to_string()
}

impl OfferingLocation {}

/// Mode-specific data as enum variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum OfferingModeData {
    /// Container managed by Moss
    Managed(ManagedData),

    /// Native service adopted by Moss
    Adopted(AdoptedData),

    /// External service announced by Moss
    Borrowed(BorrowedData),
}

impl OfferingModeData {
    /// Get the offering mode
    pub fn mode(&self) -> OfferingMode {
        match self {
            Self::Managed(_) => OfferingMode::Managed,
            Self::Adopted(_) => OfferingMode::Adopted,
            Self::Borrowed(_) => OfferingMode::Borrowed,
        }
    }
}

/// Data specific to managed (container) offerings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManagedData {
    /// Container resources (CPU, memory usage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ContainerResources>,

    /// Job ID for tracking installation progress
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,

    /// Cached post-installation guidance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<OfferingGuidance>,
}

/// Data specific to adopted (native) offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptedData {
    /// Control level (Full/Monitor/Announce)
    #[serde(default)]
    pub control_level: AdoptedControlLevel,

    /// Start command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_command: Option<String>,

    /// Stop command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,

    /// Restart command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_command: Option<String>,

    /// Health check URL for HTTP health probes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_url: Option<String>,

    /// Cached post-adoption guidance (if available)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub guidance: Option<OfferingGuidance>,

    /// Container name if adopted from a container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,

    /// When the service was detected
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

/// Data specific to borrowed (external) offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowedData {
    /// Health check method (Http/Tcp/None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_method: Option<HealthMethod>,

    /// Key to retrieve credentials from secrets store
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials_key: Option<String>,

    /// Connection string template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_template: Option<String>,

    /// When this service was announced
    pub announced_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Offering Helper Methods
// ============================================================================

impl Offering {
    /// Get the offering mode
    pub fn mode(&self) -> OfferingMode {
        self.mode_data.mode()
    }

    /// Check if this is a managed container
    pub fn is_managed(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Managed(_))
    }

    /// Check if this is an adopted service
    pub fn is_adopted(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Adopted(_))
    }

    /// Check if this is a borrowed service
    pub fn is_borrowed(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Borrowed(_))
    }

    /// Get managed-specific data (if managed)
    pub fn managed_data(&self) -> Option<&ManagedData> {
        match &self.mode_data {
            OfferingModeData::Managed(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable managed-specific data (if managed)
    pub fn managed_data_mut(&mut self) -> Option<&mut ManagedData> {
        match &mut self.mode_data {
            OfferingModeData::Managed(data) => Some(data),
            _ => None,
        }
    }

    /// Get adopted-specific data (if adopted)
    pub fn adopted_data(&self) -> Option<&AdoptedData> {
        match &self.mode_data {
            OfferingModeData::Adopted(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable adopted-specific data (if adopted)
    pub fn adopted_data_mut(&mut self) -> Option<&mut AdoptedData> {
        match &mut self.mode_data {
            OfferingModeData::Adopted(data) => Some(data),
            _ => None,
        }
    }

    /// Get borrowed-specific data (if borrowed)
    pub fn borrowed_data(&self) -> Option<&BorrowedData> {
        match &self.mode_data {
            OfferingModeData::Borrowed(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable borrowed-specific data (if borrowed)
    pub fn borrowed_data_mut(&mut self) -> Option<&mut BorrowedData> {
        match &mut self.mode_data {
            OfferingModeData::Borrowed(data) => Some(data),
            _ => None,
        }
    }

    /// Update the timestamp
    pub fn touch(&mut self) {
        self.updated_at = Some(chrono::Utc::now());
    }
}

// ============================================================================
// Offering Guidance Types
// ============================================================================

/// Offering guidance - post-installation notes displayed on the stone portrait
///
/// Guidance is markdown content with YAML frontmatter that provides
/// helpful information to users after an offering is installed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OfferingGuidance {
    /// Raw markdown content (without frontmatter)
    pub content: String,

    /// Variables that have been substituted (for reference)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub variables: std::collections::HashMap<String, String>,
}

/// Guidance frontmatter metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceFrontmatter {
    /// Schema version
    #[serde(default = "default_version")]
    pub version: String,

    /// When to show the guidance (default: post_install)
    #[serde(default)]
    pub trigger: GuidanceTrigger,
}

fn default_version() -> String {
    "1".to_string()
}

impl Default for GuidanceFrontmatter {
    fn default() -> Self {
        Self {
            version: default_version(),
            trigger: GuidanceTrigger::default(),
        }
    }
}

/// When guidance should be displayed
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceTrigger {
    /// Show after installation (default)
    #[default]
    PostInstall,
    /// Always show while offering is running
    Always,
}

// ============================================================================
// Scheduled Task Types (Maintenance, Periodic Operations)
// ============================================================================

/// Category of scheduled task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TaskCategory {
    /// Maintenance tasks (updates, cleanup, optimization)
    #[default]
    Maintenance,
    /// Backup operations
    Backup,
    /// Health/monitoring tasks
    Health,
    /// Custom/other tasks
    Custom,
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Maintenance => write!(f, "maintenance"),
            Self::Backup => write!(f, "backup"),
            Self::Health => write!(f, "health"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Task definition in a manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Human-readable description
    pub description: String,

    /// Cron schedule expression (e.g., "0 3 * * *" for daily at 3 AM)
    pub schedule: String,

    /// Command to execute inside the container
    pub command: Vec<String>,

    /// Task category (default: maintenance)
    #[serde(default)]
    pub category: TaskCategory,

    /// Whether task is enabled (default: true)
    #[serde(default = "default_task_enabled")]
    pub enabled: bool,

    /// Timeout in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_task_timeout")]
    pub timeout_secs: u64,
}

fn default_task_enabled() -> bool {
    true
}

fn default_task_timeout() -> u64 {
    300
}

/// Scheduled task instance for a specific offering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// Unique task ID (offering_id + task_name)
    pub task_id: String,

    /// Offering ID this task belongs to
    pub offering_id: String,

    /// Offering name (for display)
    pub offering_name: String,

    /// Task name (key from manifest)
    pub task_name: String,

    /// Task definition
    pub definition: TaskDefinition,

    /// When this task was registered
    pub registered_at: String,

    /// Last execution time (if any)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_run: Option<String>,

    /// Last execution result
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_result: Option<TaskResult>,

    /// Next scheduled run time
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next_run: Option<String>,
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Whether the task succeeded
    pub success: bool,

    /// Exit code (if available)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Output (truncated if too long)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output: Option<String>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

// ============================================================================
// API Error Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_status_serde() {
        let status = ServiceStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ServiceStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_service_health_status_serde() {
        let health = ServiceHealthStatus::Healthy;
        let json = serde_json::to_string(&health).unwrap();
        let deserialized: ServiceHealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(health, deserialized);
    }

    #[test]
    fn test_service_info_serde() {
        let info = ServiceInfo {
            offering_id: "018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f".into(),
            name: "mongodb".into(),
            offering: "mongodb".into(),
            version: "7.0".into(),
            status: ServiceStatus::Running,
            health: ServiceHealthStatus::Healthy,
            ports: Ports {
                native: 27017,
                agnostic: Some(8080),
            },
            resources: None,
            job_id: None,
            sub_capabilities: Vec::new(),
            guidance: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.name, deserialized.name);
        assert_eq!(info.status, deserialized.status);
        assert_eq!(info.offering_id, deserialized.offering_id);
    }

    #[test]
    fn test_service_info_offering_id_migration() {
        // Test that existing services without offering_id deserialize correctly
        // (serde default should provide empty string)
        let json = r#"{
            "name": "mongodb",
            "offering": "mongodb",
            "version": "7.0",
            "status": "Running",
            "health": "Healthy",
            "ports": {"native": 27017, "agnostic": 8080}
        }"#;
        let deserialized: ServiceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.offering_id, "");
        assert_eq!(deserialized.name, "mongodb");
    }

    #[test]
    fn test_discovery_request_serde() {
        let req = DiscoveryRequest {
            discover: "moss".into(),
            request_id: "test-123".into(),
            requester: "rake".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: DiscoveryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.discover, deserialized.discover);
        assert_eq!(req.request_id, deserialized.request_id);
    }

    #[test]
    fn test_discovery_response_serde() {
        let resp = DiscoveryResponse {
            stone_id: Some("01234567-89ab-cdef-0123-456789abcdef".into()),
            stone_name: "stone-01".into(),
            address: crate::PeerAddress::new("127.0.0.1".parse().unwrap(), 3001),
            moss_version: "0.1.0".into(),
            lantern_endpoint: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: DiscoveryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.stone_name, deserialized.stone_name);
    }

    #[test]
    fn test_pond_config_defaults() {
        let config = PondConfig {
            enabled: false,
            keystone_path: None,
            require_mtls: false,
        };
        assert!(!config.enabled);
        assert!(!config.require_mtls);
    }

    #[test]
    fn test_stone_invite_request() {
        let req = StoneInviteRequest {
            stone_name: "stone-02".into(),
            expiry_hours: Some(24),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("stone-02"));
    }

    #[test]
    fn test_offering_mode_serde() {
        let mode = OfferingMode::Adopted;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"adopted\"");
        let deserialized: OfferingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }

    #[test]
    fn test_adopted_control_level_default() {
        let default = AdoptedControlLevel::default();
        assert_eq!(default, AdoptedControlLevel::Monitor);
    }

    #[test]
    fn test_adopted_control_level_serde() {
        let level = AdoptedControlLevel::Full;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"full\"");
        let deserialized: AdoptedControlLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(level, deserialized);
    }

    #[test]
    fn test_health_method_serde() {
        let method = HealthMethod::Http;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, "\"http\"");
        let deserialized: HealthMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(method, deserialized);
    }
}
