//! Fact resolution for predicate evaluation (COMPAT-0002).
//!
//! The `FactSource` trait resolves dotted fact names directly against
//! hardware data. `HardwareCapabilities` implements it — no intermediate
//! struct, no conversion step.

use super::predicate::Fact;
use std::collections::HashSet;

/// Resolves facts for predicate evaluation.
///
/// Implementors map the `host.*` fact namespace to their internal structure.
/// The predicate evaluator calls these methods — one per fact type.
pub trait FactSource {
    fn resolve_set(&self, fact: Fact) -> HashSet<String>;
    fn resolve_scalar(&self, fact: Fact) -> Option<String>;
    fn resolve_numeric(&self, fact: Fact) -> f64;
    fn resolve_bool(&self, fact: Fact) -> bool;
}

/// Known AI runtime names (filtered from gpu.capabilities).
const AI_RUNTIME_NAMES: &[&str] = &["cuda", "rocm", "directml", "openvino"];

impl FactSource for crate::types::hardware::HardwareCapabilities {
    fn resolve_set(&self, fact: Fact) -> HashSet<String> {
        match fact {
            Fact::AiRuntime => {
                // Derive from gpu.capabilities — no ai_runtimes field needed
                self.hardware
                    .gpus
                    .iter()
                    .flat_map(|g| &g.capabilities)
                    .filter(|c| AI_RUNTIME_NAMES.contains(&c.to_lowercase().as_str()))
                    .map(|c| c.to_lowercase())
                    .collect()
            }
            Fact::CpuFeatures => self
                .hardware
                .cpu
                .features
                .as_ref()
                .map(|feats| feats.iter().map(|f| f.to_lowercase()).collect())
                .unwrap_or_default(),
            Fact::CpuPattern => self
                .hardware
                .cpu
                .model
                .as_deref()
                .map(extract_cpu_patterns)
                .unwrap_or_default(),
            _ => HashSet::new(),
        }
    }

    fn resolve_scalar(&self, fact: Fact) -> Option<String> {
        match fact {
            Fact::Architecture => Some(self.hardware.cpu.architecture.clone()),
            Fact::OsFamily => self.runtime.as_ref().map(|r| {
                // runtime.os may be "windows/Microsoft Windows 11 Pro" — extract family
                r.os.split('/').next().unwrap_or(&r.os).to_lowercase()
            }),
            Fact::CpuModel => self.hardware.cpu.model.clone(),
            _ => None,
        }
    }

    fn resolve_numeric(&self, fact: Fact) -> f64 {
        match fact {
            Fact::RamTotalMb => self.hardware.memory.total_mb as f64,
            Fact::GpuCount => self.hardware.gpus.len() as f64,
            Fact::GpuVramTotalMb => {
                self.hardware
                    .gpus
                    .iter()
                    .filter_map(|g| g.vram_mb)
                    .sum::<u64>() as f64
            }
            Fact::GpuVramTotalGb => {
                let mb: u64 = self
                    .hardware
                    .gpus
                    .iter()
                    .filter_map(|g| g.vram_mb)
                    .sum();
                mb as f64 / 1024.0
            }
            _ => 0.0,
        }
    }

    fn resolve_bool(&self, fact: Fact) -> bool {
        match fact {
            Fact::Gpu => !self.hardware.gpus.is_empty(),
            Fact::Npu => false, // TODO: detect NPU when hardware support is added
            _ => false,
        }
    }
}

/// Extract matchable CPU pattern identifiers from a model string.
///
/// Lowercases the model and extracts tokens that contain digits
/// (e.g., "j4105" from "Intel Celeron J4105").
fn extract_cpu_patterns(model: &str) -> HashSet<String> {
    let lower = model.to_lowercase();
    lower
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '@')
        .filter(|t| !t.is_empty() && t.chars().any(|c| c.is_ascii_digit()))
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cpu_patterns_celeron() {
        let patterns = extract_cpu_patterns("Intel Celeron J4105");
        assert!(patterns.contains("j4105"));
        assert!(!patterns.contains("intel"));
        assert!(!patterns.contains("celeron"));
    }

    #[test]
    fn extract_cpu_patterns_ryzen() {
        let patterns = extract_cpu_patterns("AMD Ryzen 7 7700 8-Core Processor");
        assert!(patterns.contains("7"));
        assert!(patterns.contains("7700"));
        assert!(patterns.contains("8"));
    }

    #[test]
    fn extract_cpu_patterns_i7() {
        let patterns = extract_cpu_patterns("12th Gen Intel(R) Core(TM) i7-12700KF");
        assert!(patterns.contains("12th"));
        assert!(patterns.contains("i7"));
        // i7 doesn't contain a digit... let me check
        // Actually "i7" has '7' which is a digit
    }
}
