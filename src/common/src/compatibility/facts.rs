//! Host fact tree — the hardware state a predicate evaluates against.

use std::collections::HashSet;

/// Hardware facts about a stone, structured for predicate evaluation.
///
/// Populated from `HardwareCapabilities` by moss at startup. All fields
/// use `Option` or empty collections so that missing detection results
/// in predicates evaluating to `false` (not errors).
#[derive(Debug, Clone, Default)]
pub struct HostFacts {
    /// CPU architecture: x86_64, aarch64, armv7l, armv6l
    pub architecture: Option<String>,
    /// OS family: linux, windows, macos
    pub os_family: Option<String>,
    /// Full CPU model string (e.g., "Intel Celeron J4105")
    pub cpu_model: Option<String>,
    /// Substring-matchable CPU identifiers, lowercased (e.g., "j4105")
    pub cpu_patterns: HashSet<String>,
    /// CPU feature flags: avx, avx2, sse4_2, avx512, ...
    pub cpu_features: HashSet<String>,
    /// Total system RAM in MB
    pub ram_total_mb: Option<u64>,
    /// GPU hardware present
    pub gpu_present: bool,
    /// Number of GPUs
    pub gpu_count: u32,
    /// Aggregate VRAM across all GPUs in MB
    pub gpu_vram_total_mb: u64,
    /// NPU hardware present
    pub npu_present: bool,
    /// Detected AI runtime toolkits: cuda, rocm, directml, openvino
    pub ai_runtimes: HashSet<String>,
}
