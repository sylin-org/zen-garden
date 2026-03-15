//! Compatibility system types — rules, conditions, healthchecks.

use serde::{Deserialize, Serialize};

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

    // GPU/AI hardware presence (simple boolean — uses same detection as `observe`)
    /// Match if stone has (true) or lacks (false) any GPU hardware.
    /// Simpler and more reliable than runtime-specific checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_gpu: Option<bool>,
    /// Match if stone has (true) or lacks (false) any AI runtime
    /// (CUDA, ROCm, DirectML, OpenVINO, NPU, tensor cores, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_ai_runtime: Option<bool>,

    // AI/GPU requirements (runtime-specific — prefer has_gpu/has_ai_runtime above)
    /// Match if ANY of the listed AI runtimes are present (OR logic: ['cuda', 'rocm'])
    /// Use for offerings that REQUIRE a specific GPU runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_ai_any: Option<Vec<String>>,
    /// Match if ALL of the listed AI runtimes are present (AND logic: ['cuda', 'directml'])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_ai_all: Option<Vec<String>>,
    /// Match if ANY of the listed AI runtimes are detected on the stone.
    /// Use for offerings that must EXCLUDE GPU-equipped stones (e.g. ollama-cpu).
    /// Semantically identical to requires_ai_any but named for the denial case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_present_any: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb_less_than: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb_at_least: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub image: String,
    /// Suggested offering instance name when this fallback applies.
    /// e.g. `"legacy"` → FQN becomes `mongodb::legacy` instead of `mongodb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
