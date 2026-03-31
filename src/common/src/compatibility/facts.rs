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

impl HostFacts {
    /// Build from cached `HardwareCapabilities` (fast path — no I/O).
    pub fn from_capabilities(caps: &crate::types::hardware::HardwareCapabilities) -> Self {
        let gpus = &caps.hardware.gpus;

        let has_runtime = |runtime_name: &str| {
            gpus.iter().any(|g| {
                g.ai_runtimes.iter().any(|r| {
                    let r_lower = r.to_lowercase();
                    let runtime_lower = runtime_name.to_lowercase();
                    r_lower == runtime_lower
                        || r_lower.starts_with(&format!("{}:", runtime_lower))
                })
            })
        };

        let mut ai_runtimes = HashSet::new();
        for name in &["cuda", "rocm", "directml", "openvino"] {
            if has_runtime(name) {
                ai_runtimes.insert(name.to_string());
            }
        }

        // Extract CPU pattern identifiers (lowercased substrings for matching)
        let cpu_patterns = caps
            .hardware
            .cpu
            .model
            .as_deref()
            .map(|model| extract_cpu_patterns(model))
            .unwrap_or_default();

        let cpu_features = caps
            .hardware
            .cpu
            .features
            .as_ref()
            .map(|feats| feats.iter().map(|f| f.to_lowercase()).collect())
            .unwrap_or_default();

        Self {
            architecture: Some(caps.hardware.cpu.architecture.clone()),
            os_family: caps.runtime.as_ref().map(|r| r.os.clone()),
            cpu_model: caps.hardware.cpu.model.clone(),
            cpu_patterns,
            cpu_features,
            ram_total_mb: Some(caps.hardware.memory.total_mb),
            gpu_present: !gpus.is_empty(),
            gpu_count: gpus.len() as u32,
            gpu_vram_total_mb: gpus.iter().filter_map(|g| g.vram_mb).sum(),
            npu_present: false, // TODO: detect NPU when hardware support is added
            ai_runtimes,
        }
    }

    /// Build from live system detection (slow path — shells out to system tools).
    pub fn from_live_detection() -> Self {
        let (cpu_model, cpu_features_vec, architecture) =
            crate::metrics::system::get_cpu_info().unwrap_or_else(|_| {
                (
                    "Unknown".to_string(),
                    vec![],
                    std::env::consts::ARCH.to_string(),
                )
            });

        let resources = crate::metrics::system::collect_stone_resources().ok();
        let ram_total_mb = resources
            .as_ref()
            .map(|r| r.memory.total_bytes / 1024 / 1024);

        let gpus = crate::metrics::system::detect_gpus();

        let has_runtime = |runtime_name: &str| {
            gpus.iter().any(|g| {
                g.ai_runtimes.iter().any(|r| {
                    let r_lower = r.to_lowercase();
                    let runtime_lower = runtime_name.to_lowercase();
                    r_lower == runtime_lower
                        || r_lower.starts_with(&format!("{}:", runtime_lower))
                })
            })
        };

        let mut ai_runtimes = HashSet::new();
        for name in &["cuda", "rocm", "directml", "openvino"] {
            if has_runtime(name) {
                ai_runtimes.insert(name.to_string());
            }
        }

        let cpu_patterns = extract_cpu_patterns(&cpu_model);
        let cpu_features = cpu_features_vec
            .iter()
            .map(|f| f.to_lowercase())
            .collect();

        Self {
            architecture: Some(architecture),
            os_family: Some(std::env::consts::OS.to_string()),
            cpu_model: Some(cpu_model),
            cpu_patterns,
            cpu_features,
            ram_total_mb,
            gpu_present: !gpus.is_empty(),
            gpu_count: gpus.len() as u32,
            gpu_vram_total_mb: gpus.iter().filter_map(|g| g.vram_mb).sum(),
            npu_present: false,
            ai_runtimes,
        }
    }

    /// Build from cached capabilities with live fallback when cache is incomplete.
    pub fn detect(cached: Option<&crate::types::hardware::HardwareCapabilities>) -> Self {
        if let Some(caps) = cached {
            if caps.detection_status == crate::DetectionStatus::Complete {
                return Self::from_capabilities(caps);
            }
        }
        Self::from_live_detection()
    }
}

/// Extract matchable CPU pattern identifiers from a model string.
///
/// Lowercases the model and extracts tokens that match known CPU pattern
/// identifiers (Celeron J/N series, Atom, etc.).
fn extract_cpu_patterns(model: &str) -> HashSet<String> {
    let lower = model.to_lowercase();
    let mut patterns = HashSet::new();

    // Extract tokens that look like CPU identifiers (alphanumeric sequences)
    for token in lower.split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '@') {
        let trimmed = token.trim();
        if !trimmed.is_empty() && trimmed.chars().any(|c| c.is_ascii_digit()) {
            patterns.insert(trimmed.to_string());
        }
    }

    patterns
}
