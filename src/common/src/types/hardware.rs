//! Hardware and resource types — dynamic resource snapshots, GPU info,
//! static capabilities. The word "metrics" in moss refers to software
//! observability (see `domain::metrics`) — hardware state lives here
//! under "resources".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiskType {
    NVMe,
    SSD,
    HDD,
    Unknown,
}

/// Per-disk storage resources (live data, collected every 30s).
///
/// Contains both device info and current usage in one structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageResources {
    pub identifier: String,  // e.g., "sda", "nvme0n1", "C:"
    pub mount_point: String, // e.g., "/", "/data", "C:\"
    pub total_gb: u64,
    pub used_gb: u64,
    pub available_gb: u64,
    pub used_percent: f32,
    pub disk_type: DiskType,
    pub filesystem: String, // e.g., "ext4", "NTFS"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
    /// Hardware capabilities: "cuda", "rocm", "vulkan", "directml", "opencl".
    /// Single source of truth for what the GPU supports.
    /// The compatibility DSL reads from this field via `host.ai.runtime`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>,
}

/// Live stone resources (collected every 5s for CPU/memory, 30s for storage).
///
/// This is the single source of truth for runtime resource snapshots.
/// Storage inventory included here due to semi-dynamic nature (hot-swap, mounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneResources {
    pub cpu: CpuResources,
    pub memory: MemoryResources,
    pub storage: Vec<StorageResources>, // All mounted disks with live usage
    pub uptime_seconds: u64,
    pub uptime_friendly: String,
    /// CPU package temperature in degrees Celsius.
    /// Available on Linux (hwmon/coretemp) and ARM stones with thermal sensors.
    /// `None` when the platform or hardware does not expose thermal data.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cpu_temperature: Option<f32>,
}

impl StoneResources {
    /// The storage mount that holds Zen Garden's data — offering data, container images,
    /// caches — i.e. the mounted filesystem whose mount point is the deepest prefix of
    /// `data_dir()`. This is the single accessor every "stone storage" figure must read
    /// (capabilities, health, portrait, presence, garden): the OS root can be a tiny system
    /// partition (e.g. ~1 GB on Android) while the data partition holds the real capacity.
    /// Falls back to the largest mount when none contains the data path.
    pub fn data_partition(&self) -> Option<&StorageResources> {
        let data_path = crate::constants::paths::data_dir();
        self.storage
            .iter()
            .filter(|s| mount_contains(&s.mount_point, &data_path))
            .max_by_key(|s| s.mount_point.len())
            .or_else(|| self.storage.iter().max_by_key(|s| s.total_gb))
    }

    /// Build the static disk capability (capacity + type) from the data partition — the single
    /// way every capabilities builder derives `DiskCapabilities`. Returns a zeroed capability
    /// (total 0, type None) when no partition is found, matching prior builder behavior.
    pub fn disk_capabilities(&self) -> DiskCapabilities {
        match self.data_partition() {
            Some(s) => DiskCapabilities {
                total_gb: s.total_gb,
                disk_type: Some(match s.disk_type {
                    DiskType::NVMe => "NVMe".to_string(),
                    DiskType::SSD => "SSD".to_string(),
                    DiskType::HDD => "HDD".to_string(),
                    DiskType::Unknown => "Unknown".to_string(),
                }),
            },
            None => DiskCapabilities {
                total_gb: 0,
                disk_type: None,
            },
        }
    }
}

/// True if filesystem `path` lives under `mount` (prefix match at a path-component boundary).
/// A mount that is empty after trimming separators (`"/"`, `"\"`) is the root — it contains
/// every path.
fn mount_contains(mount: &str, path: &str) -> bool {
    let m = mount.trim_end_matches(['/', '\\']);
    m.is_empty()
        || path == m
        || path.starts_with(&format!("{m}/"))
        || path.starts_with(&format!("{m}\\"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuResources {
    pub cores: usize,
    pub usage_percent: f32,
    pub usage_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResources {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f32,
    pub total_friendly: String,
    pub used_friendly: String,
    pub available_friendly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskResources {
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
pub struct ResourcesSnapshot {
    pub timestamp: String,
    pub cpu: CpuResources,
    pub memory: MemoryResources,
    pub disk: DiskResources,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkResources>,
    pub uptime_seconds: u64,
}

/// Network resources for all interfaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkResources {
    /// Per-interface statistics
    pub interfaces: Vec<InterfaceResources>,
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

/// Per-interface network resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceResources {
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
