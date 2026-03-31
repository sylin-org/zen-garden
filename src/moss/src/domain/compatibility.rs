//! Compatibility evaluation and binary validation (COMPAT-0002)
//!
//! Pure business logic for:
//! - Predicate DSL evaluation against host hardware facts
//! - Compatibility rule compilation for offerings index
//! - Binary architecture validation
//!
//! No I/O here — delegates to `HostFacts` for detection.

use anyhow::Result;
use garden_common::compatibility::{HostFacts, Predicate};
use garden_common::HardwareCapabilities;

/// Result of compatibility evaluation
#[derive(Debug, Clone)]
pub enum CompatibilityDecision {
    Pass,
    Fallback {
        image: String,
        name: Option<String>,
        reason: String,
    },
    Warning {
        reason: String,
        suggestion: Option<String>,
    },
    Fail {
        reason: String,
        suggestion: Option<String>,
    },
}

/// Compiled compatibility result (serializable for API responses)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompiledCompatibility {
    pub decision: String, // "pass" | "fallback" | "warning" | "fail"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Build host facts from cached capabilities with live fallback.
pub fn get_host_facts(cached: Option<&HardwareCapabilities>) -> HostFacts {
    HostFacts::detect(cached)
}

/// Compile compatibility rules for a template.
///
/// Evaluates rules against host facts and modifies template image if a
/// fallback applies. Returns structured compatibility result for the
/// offerings index.
pub fn compile_compatibility(
    template: &mut garden_common::manifests::ServiceTemplate,
    cached_capabilities: Option<&HardwareCapabilities>,
) -> CompiledCompatibility {
    if let Some(rules) = &template.compatibility {
        let host = get_host_facts(cached_capabilities);
        match evaluate_compatibility(rules, &host) {
            CompatibilityDecision::Pass => CompiledCompatibility {
                decision: garden_common::constants::COMPAT_PASS.to_string(),
                reason: None,
                original_image: None,
                fallback_image: None,
                fallback_name: None,
                suggestion: None,
            },
            CompatibilityDecision::Fallback {
                image,
                name,
                reason,
            } => {
                let original_image = template.image.clone();
                template.image = image.clone();
                CompiledCompatibility {
                    decision: garden_common::constants::COMPAT_FALLBACK.to_string(),
                    reason: Some(reason),
                    original_image: Some(original_image),
                    fallback_image: Some(image),
                    fallback_name: name,
                    suggestion: None,
                }
            }
            CompatibilityDecision::Warning { reason, suggestion } => CompiledCompatibility {
                decision: garden_common::constants::COMPAT_WARNING.to_string(),
                reason: Some(reason),
                original_image: None,
                fallback_image: None,
                fallback_name: None,
                suggestion,
            },
            CompatibilityDecision::Fail { reason, suggestion } => CompiledCompatibility {
                decision: garden_common::constants::COMPAT_FAIL.to_string(),
                reason: Some(reason),
                original_image: Some(template.image.clone()),
                fallback_image: None,
                fallback_name: None,
                suggestion,
            },
        }
    } else {
        CompiledCompatibility {
            decision: garden_common::constants::COMPAT_PASS.to_string(),
            reason: None,
            original_image: None,
            fallback_image: None,
            fallback_name: None,
            suggestion: None,
        }
    }
}

/// Evaluate compatibility rules against host facts using the predicate DSL.
///
/// Rules are evaluated in order. Within a rule, all `when` predicates must
/// match (AND semantics). First matching rule wins, unless it has
/// `continue_eval: true` (warnings that don't short-circuit).
pub fn evaluate_compatibility(
    rules: &garden_common::CompatibilityRules,
    host: &HostFacts,
) -> CompatibilityDecision {
    let mut last_warning: Option<CompatibilityDecision> = None;

    for rule in &rules.compatibility_rules {
        // Parse and evaluate all predicates in the `when` clause
        let predicates: Vec<Predicate> = match rule
            .when
            .iter()
            .map(|s| Predicate::parse(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(preds) => preds,
            Err(e) => {
                tracing::warn!(
                    rule = %rule.name,
                    error = %e,
                    "Failed to parse compatibility predicate — skipping rule"
                );
                continue;
            }
        };

        // Empty `when` matches everything (catch-all rule)
        let matches = if predicates.is_empty() {
            true
        } else {
            garden_common::compatibility::check_all(&predicates, host)
        };

        if matches {
            if let Some(fallback) = &rule.fallback {
                return CompatibilityDecision::Fallback {
                    image: fallback.image.clone(),
                    name: fallback.name.clone(),
                    reason: rule.reason.clone(),
                };
            }

            if rule.warn_only {
                let warning = CompatibilityDecision::Warning {
                    reason: rule.reason.clone(),
                    suggestion: rule.suggestion.clone(),
                };

                if rule.continue_eval {
                    // Stash warning but keep evaluating subsequent rules
                    last_warning = Some(warning);
                    continue;
                }

                return warning;
            }

            return CompatibilityDecision::Fail {
                reason: rule.reason.clone(),
                suggestion: rule.suggestion.clone(),
            };
        }
    }

    // If we collected a continued warning but no harder decision, return it
    last_warning.unwrap_or(CompatibilityDecision::Pass)
}

/// Validate ELF binary architecture matches system
///
/// Returns the detected architecture or an error if validation fails.
pub fn validate_binary_architecture(binary_data: &[u8]) -> Result<String> {
    use anyhow::bail;

    if binary_data.len() < 20 {
        bail!("Binary too small (expected at least 20 bytes for ELF header)");
    }

    if &binary_data[0..4] != b"\x7fELF" {
        bail!("Not a valid ELF binary (invalid magic bytes)");
    }

    let machine_type = u16::from_le_bytes([binary_data[0x12], binary_data[0x13]]);
    let arch = match machine_type {
        0x3E => "x86_64",
        0xB7 => "aarch64",
        0x28 => "arm",
        _ => bail!("Unsupported architecture: machine type {:#x}", machine_type),
    };

    let system_arch = std::env::consts::ARCH;
    if arch != system_arch {
        bail!(
            "Architecture mismatch: binary is {}, but system is {}",
            arch,
            system_arch
        );
    }

    Ok(arch.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn rocm_host() -> HostFacts {
        HostFacts {
            architecture: Some("x86_64".into()),
            os_family: Some("linux".into()),
            cpu_model: Some("AMD Ryzen 9".into()),
            cpu_features: HashSet::from(["avx2".into(), "sse4_2".into()]),
            ram_total_mb: Some(32768),
            gpu_present: true,
            gpu_count: 1,
            gpu_vram_total_mb: 8192,
            ai_runtimes: HashSet::from(["rocm".into()]),
            ..Default::default()
        }
    }

    fn nvidia_host() -> HostFacts {
        HostFacts {
            architecture: Some("x86_64".into()),
            os_family: Some("linux".into()),
            cpu_features: HashSet::from(["avx2".into()]),
            ram_total_mb: Some(16384),
            gpu_present: true,
            gpu_count: 1,
            gpu_vram_total_mb: 8192,
            ai_runtimes: HashSet::from(["cuda".into()]),
            ..Default::default()
        }
    }

    fn cpu_only_host() -> HostFacts {
        HostFacts {
            architecture: Some("x86_64".into()),
            os_family: Some("linux".into()),
            cpu_model: Some("Intel Celeron J4105".into()),
            cpu_patterns: HashSet::from(["j4105".into()]),
            cpu_features: HashSet::from(["sse4_2".into()]),
            ram_total_mb: Some(8192),
            ..Default::default()
        }
    }

    fn comfyui_rules() -> garden_common::CompatibilityRules {
        garden_common::CompatibilityRules {
            compatibility_rules: vec![
                garden_common::CompatibilityRule {
                    name: "no-nvidia-use-rocm".into(),
                    when: vec![
                        "host.ai.runtime LACKS cuda".into(),
                        "host.ai.runtime HAS rocm".into(),
                    ],
                    reason: "No NVIDIA GPU. Using AMD ROCm image.".into(),
                    suggestion: None,
                    fallback: Some(garden_common::FallbackConfig {
                        image: "yanwk/comfyui-boot:rocm".into(),
                        name: Some("rocm".into()),
                    }),
                    warn_only: false,
                    continue_eval: false,
                },
                garden_common::CompatibilityRule {
                    name: "no-gpu-use-cpu".into(),
                    when: vec!["host.ai.runtime LACKS cuda".into()],
                    reason: "No GPU. CPU mode.".into(),
                    suggestion: None,
                    fallback: Some(garden_common::FallbackConfig {
                        image: "yanwk/comfyui-boot:cpu".into(),
                        name: Some("cpu".into()),
                    }),
                    warn_only: false,
                    continue_eval: false,
                },
            ],
            post_install_healthcheck: None,
        }
    }

    #[test]
    fn comfyui_nvidia_passes() {
        let result = evaluate_compatibility(&comfyui_rules(), &nvidia_host());
        assert!(matches!(result, CompatibilityDecision::Pass));
    }

    #[test]
    fn comfyui_rocm_falls_back() {
        let result = evaluate_compatibility(&comfyui_rules(), &rocm_host());
        match result {
            CompatibilityDecision::Fallback { image, name, .. } => {
                assert_eq!(image, "yanwk/comfyui-boot:rocm");
                assert_eq!(name.as_deref(), Some("rocm"));
            }
            other => panic!("Expected Fallback, got {:?}", other),
        }
    }

    #[test]
    fn comfyui_no_gpu_falls_back_to_cpu() {
        let result = evaluate_compatibility(&comfyui_rules(), &cpu_only_host());
        match result {
            CompatibilityDecision::Fallback { image, name, .. } => {
                assert_eq!(image, "yanwk/comfyui-boot:cpu");
                assert_eq!(name.as_deref(), Some("cpu"));
            }
            other => panic!("Expected Fallback, got {:?}", other),
        }
    }

    #[test]
    fn empty_rules_pass() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![],
            post_install_healthcheck: None,
        };
        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(matches!(result, CompatibilityDecision::Pass));
    }

    #[test]
    fn warn_only_rule() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![garden_common::CompatibilityRule {
                name: "low-vram".into(),
                when: vec!["host.gpu.vram.total.mb < 4096".into()],
                reason: "Low VRAM".into(),
                suggestion: Some("Use smaller models".into()),
                fallback: None,
                warn_only: true,
                continue_eval: false,
            }],
            post_install_healthcheck: None,
        };
        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(matches!(result, CompatibilityDecision::Warning { .. }));
    }

    #[test]
    fn continue_eval_does_not_short_circuit() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![
                garden_common::CompatibilityRule {
                    name: "low-vram-warning".into(),
                    when: vec!["host.gpu.vram.total.mb < 4096".into()],
                    reason: "Low VRAM".into(),
                    suggestion: None,
                    fallback: None,
                    warn_only: true,
                    continue_eval: true, // Don't stop here
                },
                garden_common::CompatibilityRule {
                    name: "no-gpu-fallback".into(),
                    when: vec!["host.ai.runtime LACKS cuda".into()],
                    reason: "No CUDA".into(),
                    suggestion: None,
                    fallback: Some(garden_common::FallbackConfig {
                        image: "cpu-image".into(),
                        name: Some("cpu".into()),
                    }),
                    warn_only: false,
                    continue_eval: false,
                },
            ],
            post_install_healthcheck: None,
        };
        // Should reach the fallback rule, not stop at warning
        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(
            matches!(result, CompatibilityDecision::Fallback { .. }),
            "Expected Fallback, got {:?}",
            result
        );
    }

    #[test]
    fn continue_eval_returns_warning_if_no_harder_decision() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![garden_common::CompatibilityRule {
                name: "low-vram-warning".into(),
                when: vec!["host.gpu.vram.total.mb < 4096".into()],
                reason: "Low VRAM".into(),
                suggestion: Some("Use smaller models".into()),
                fallback: None,
                warn_only: true,
                continue_eval: true,
            }],
            post_install_healthcheck: None,
        };
        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(matches!(result, CompatibilityDecision::Warning { .. }));
    }

    #[test]
    fn gpu_present_blocks_cpu_offering() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![garden_common::CompatibilityRule {
                name: "gpu-present".into(),
                when: vec!["host.gpu IS present".into()],
                reason: "GPU detected, use GPU offering".into(),
                suggestion: Some("Use ollama instead".into()),
                fallback: None,
                warn_only: false,
                continue_eval: false,
            }],
            post_install_healthcheck: None,
        };
        let result = evaluate_compatibility(&rules, &nvidia_host());
        assert!(matches!(result, CompatibilityDecision::Fail { .. }));

        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(matches!(result, CompatibilityDecision::Pass));
    }

    #[test]
    fn malformed_predicate_skips_rule() {
        let rules = garden_common::CompatibilityRules {
            compatibility_rules: vec![garden_common::CompatibilityRule {
                name: "bad-rule".into(),
                when: vec!["host.nonexistent LACKS foo".into()],
                reason: "Should not fire".into(),
                suggestion: None,
                fallback: None,
                warn_only: false,
                continue_eval: false,
            }],
            post_install_healthcheck: None,
        };
        let result = evaluate_compatibility(&rules, &cpu_only_host());
        assert!(matches!(result, CompatibilityDecision::Pass));
    }

    #[test]
    fn validate_binary_architecture_valid_elf() {
        let mut header = vec![0u8; 20];
        header[0..4].copy_from_slice(b"\x7fELF");
        header[0x12] = 0x3E; // x86_64
        header[0x13] = 0x00;

        let result = validate_binary_architecture(&header);
        if std::env::consts::ARCH == "x86_64" {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn validate_binary_architecture_invalid_magic() {
        let header = vec![0u8; 20];
        assert!(validate_binary_architecture(&header).is_err());
    }
}
