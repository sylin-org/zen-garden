//! Hardware constraint validation for offering updates
//!
//! Pure business logic for validating hardware requirements against
//! detected capabilities. No I/O - uses HardwareCapabilities from common types.

use garden_common::HardwareCapabilities;

/// Hardware requirements for an offering or firmware update
#[derive(Debug, Clone)]
pub struct Requirements {
    pub cpu_features: Vec<String>,
    pub min_memory_mb: Option<u64>,
    pub architectures: Vec<String>,
}

impl Requirements {
    pub fn new() -> Self {
        Self {
            cpu_features: Vec::new(),
            min_memory_mb: None,
            architectures: Vec::new(),
        }
    }

    /// Add required CPU feature (e.g., "avx", "sse4_2")
    pub fn require_cpu_feature(mut self, feature: impl Into<String>) -> Self {
        self.cpu_features.push(feature.into());
        self
    }

    /// Set minimum memory requirement in MB
    pub fn require_memory_mb(mut self, mb: u64) -> Self {
        self.min_memory_mb = Some(mb);
        self
    }

    /// Add supported architecture (e.g., "x86_64", "aarch64")
    pub fn require_architecture(mut self, arch: impl Into<String>) -> Self {
        self.architectures.push(arch.into());
        self
    }
}

impl Default for Requirements {
    fn default() -> Self {
        Self::new()
    }
}

/// Constraint violation with human-readable reason
#[derive(Debug, Clone)]
pub enum ConstraintViolation {
    MissingCpuFeature {
        required: String,
        cpu_model: String,
    },
    InsufficientMemory {
        required: u64,
        available: u64,
    },
    IncompatibleArchitecture {
        required: Vec<String>,
        current: String,
    },
}

impl ConstraintViolation {
    /// Get human-readable error message
    pub fn message(&self) -> String {
        match self {
            ConstraintViolation::MissingCpuFeature { required, cpu_model } => {
                format!("Requires {} (CPU: {})", required.to_uppercase(), cpu_model)
            }
            ConstraintViolation::InsufficientMemory { required, available } => {
                format!(
                    "Requires {}GB memory (Available: {}GB)",
                    required / 1024,
                    available / 1024
                )
            }
            ConstraintViolation::IncompatibleArchitecture { required, current } => {
                format!(
                    "Requires {} (Current: {})",
                    required.join(" or "),
                    current
                )
            }
        }
    }
}

/// Check if hardware meets requirements
///
/// Returns Ok(()) if all constraints satisfied, Err(ConstraintViolation) otherwise.
pub fn check_constraints(
    requirements: &Requirements,
    hardware: &HardwareCapabilities,
) -> Result<(), ConstraintViolation> {
    // Check architecture
    if !requirements.architectures.is_empty() {
        let current_arch = &hardware.hardware.cpu.architecture;
        let arch_match = requirements
            .architectures
            .iter()
            .any(|req_arch| req_arch.eq_ignore_ascii_case(current_arch));

        if !arch_match {
            return Err(ConstraintViolation::IncompatibleArchitecture {
                required: requirements.architectures.clone(),
                current: current_arch.clone(),
            });
        }
    }

    // Check memory
    if let Some(required_mb) = requirements.min_memory_mb {
        let available_mb = hardware.hardware.memory.total_mb;
        if available_mb < required_mb {
            return Err(ConstraintViolation::InsufficientMemory {
                required: required_mb,
                available: available_mb,
            });
        }
    }

    // Check CPU features
    if !requirements.cpu_features.is_empty() {
        let detected_features = hardware
            .hardware
            .cpu
            .features
            .as_ref()
            .map(|f| f.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        let cpu_model = hardware
            .hardware
            .cpu
            .model
            .as_deref()
            .unwrap_or("Unknown");

        for required_feature in &requirements.cpu_features {
            // Normalize feature names (handle both "sse4_2" and "sse4.2" formats)
            let normalized_required = required_feature.replace('.', "_").to_lowercase();

            let feature_found = detected_features.iter().any(|detected| {
                let normalized_detected = detected.replace('.', "_").to_lowercase();
                normalized_detected == normalized_required
            });

            if !feature_found {
                return Err(ConstraintViolation::MissingCpuFeature {
                    required: required_feature.clone(),
                    cpu_model: cpu_model.to_string(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::{CpuCapabilities, HardwareInventory, MemoryCapabilities, DetectionStatus};

    fn mock_hardware(
        cpu_features: Vec<String>,
        memory_mb: u64,
        architecture: &str,
        cpu_model: &str,
    ) -> HardwareCapabilities {
        HardwareCapabilities {
            stone_id: Some("test-stone".to_string()),
            stone_name: "test".to_string(),
            hardware: HardwareInventory {
                cpu: CpuCapabilities {
                    model: Some(cpu_model.to_string()),
                    cores: 4,
                    threads: None,
                    architecture: architecture.to_string(),
                    features: Some(cpu_features),
                },
                memory: MemoryCapabilities { total_mb: memory_mb },
                gpus: vec![],
                disk: None,
                storage: vec![],
                os_version: None,
                kernel_version: None,
                swap_mb: None,
                ai_capabilities: None,
            },
            runtime: None,
            detection_status: DetectionStatus::Complete,
        }
    }

    #[test]
    fn test_constraints_pass_all() {
        let hardware = mock_hardware(
            vec!["avx".to_string(), "sse4_2".to_string()],
            8192,
            "x86_64",
            "Intel Core i5",
        );

        let requirements = Requirements::new()
            .require_cpu_feature("avx")
            .require_memory_mb(4096)
            .require_architecture("x86_64");

        assert!(check_constraints(&requirements, &hardware).is_ok());
    }

    #[test]
    fn test_constraints_missing_cpu_feature() {
        let hardware = mock_hardware(
            vec!["sse4_2".to_string()], // No AVX
            8192,
            "x86_64",
            "Intel Celeron J4105",
        );

        let requirements = Requirements::new().require_cpu_feature("avx");

        let result = check_constraints(&requirements, &hardware);
        assert!(result.is_err());

        if let Err(ConstraintViolation::MissingCpuFeature { required, cpu_model }) = result {
            assert_eq!(required, "avx");
            assert_eq!(cpu_model, "Intel Celeron J4105");
        } else {
            panic!("Expected MissingCpuFeature error");
        }
    }

    #[test]
    fn test_constraints_insufficient_memory() {
        let hardware = mock_hardware(vec![], 4096, "x86_64", "Intel Core i5");

        let requirements = Requirements::new().require_memory_mb(8192);

        let result = check_constraints(&requirements, &hardware);
        assert!(result.is_err());

        if let Err(ConstraintViolation::InsufficientMemory { required, available }) = result {
            assert_eq!(required, 8192);
            assert_eq!(available, 4096);
        } else {
            panic!("Expected InsufficientMemory error");
        }
    }

    #[test]
    fn test_constraints_incompatible_architecture() {
        let hardware = mock_hardware(vec![], 8192, "aarch64", "Apple M1");

        let requirements = Requirements::new().require_architecture("x86_64");

        let result = check_constraints(&requirements, &hardware);
        assert!(result.is_err());

        if let Err(ConstraintViolation::IncompatibleArchitecture { required, current }) = result {
            assert_eq!(required, vec!["x86_64"]);
            assert_eq!(current, "aarch64");
        } else {
            panic!("Expected IncompatibleArchitecture error");
        }
    }

    #[test]
    fn test_constraints_feature_normalization() {
        // Test that "sse4.2" matches "sse4_2"
        let hardware = mock_hardware(
            vec!["sse4_2".to_string()],
            8192,
            "x86_64",
            "Intel Core i5",
        );

        let requirements = Requirements::new().require_cpu_feature("sse4.2");

        assert!(check_constraints(&requirements, &hardware).is_ok());
    }

    #[test]
    fn test_constraints_no_requirements() {
        let hardware = mock_hardware(vec![], 2048, "x86_64", "Intel Celeron");

        let requirements = Requirements::new();

        assert!(check_constraints(&requirements, &hardware).is_ok());
    }

    #[test]
    fn test_constraint_violation_messages() {
        let missing_feature = ConstraintViolation::MissingCpuFeature {
            required: "avx".to_string(),
            cpu_model: "Intel Celeron J4105".to_string(),
        };
        assert!(missing_feature.message().contains("AVX"));
        assert!(missing_feature.message().contains("Celeron J4105"));

        let insufficient_memory = ConstraintViolation::InsufficientMemory {
            required: 8192,
            available: 4096,
        };
        assert!(insufficient_memory.message().contains("8GB"));
        assert!(insufficient_memory.message().contains("4GB"));

        let incompatible_arch = ConstraintViolation::IncompatibleArchitecture {
            required: vec!["x86_64".to_string()],
            current: "aarch64".to_string(),
        };
        assert!(incompatible_arch.message().contains("x86_64"));
        assert!(incompatible_arch.message().contains("aarch64"));
    }
}
