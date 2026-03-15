//! Hardware and resource types — system metrics, GPU info, capabilities.

use serde::{Deserialize, Serialize};

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
