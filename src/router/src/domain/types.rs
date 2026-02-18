//! Core domain types for the AI Capability Router.
//!
//! Pure data structures — no I/O, no async. Every type here can be
//! tested with simple unit tests.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

// ── Ollama Instance ──────────────────────────────────────────────

/// A discovered Ollama instance and its hardware profile.
#[derive(Debug, Clone)]
pub struct OllamaInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String, // e.g. "http://192.168.1.50:11434"
    pub ollama_version: Option<String>,
    pub gpu_name: Option<String>,
    pub vram_total_bytes: u64,
    pub vram_budget_bytes: u64,
    pub health: InstanceHealth,
    pub models_loaded: Vec<LoadedModel>,
    pub models_available: Vec<String>,
    pub queue_depth: u32,
    pub last_seen: Instant,
    pub last_profiled: Instant,
}

/// Instance health as observed by the router.
#[derive(Debug, Clone, PartialEq)]
pub enum InstanceHealth {
    /// Just discovered, profiling in progress.
    Profiling,
    /// Responding normally.
    Healthy,
    /// Unreachable or erroring — removed from routing pool.
    Unhealthy { since: Instant, reason: String },
}

impl InstanceHealth {
    pub fn is_routable(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// A model currently loaded in VRAM on an instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    pub name: String,
    /// Exact VRAM consumption from `/api/ps` (bytes).
    pub size_vram: u64,
    /// When Ollama will auto-unload (ISO-8601).
    pub expires_at: Option<String>,
}

// ── Model Info ───────────────────────────────────────────────────

/// Model metadata gathered from `/api/tags` and `/api/show`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub parameter_count: Option<u64>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub family: Option<String>,
    pub families: Vec<String>,
    pub capabilities: Vec<String>,
    pub size_disk: u64,
    /// Best-known VRAM estimate in bytes.
    /// Authoritative when sourced from `size_vram` (loaded model).
    /// Estimated from parameter_count + quantization otherwise.
    pub vram_estimate_bytes: u64,
}

impl ModelInfo {
    /// Estimate VRAM from parameter count and quantization level.
    /// Formula: params × bits_per_weight / 8, plus ~10 % overhead for KV cache.
    pub fn estimate_vram(parameter_count: u64, quant: Option<&str>) -> u64 {
        let bits = match quant {
            Some(q) if q.starts_with("Q2") => 2.5,
            Some(q) if q.starts_with("Q3") => 3.5,
            Some(q) if q.starts_with("Q4") => 4.5,
            Some(q) if q.starts_with("Q5") => 5.5,
            Some(q) if q.starts_with("Q6") => 6.5,
            Some(q) if q.starts_with("Q8") => 8.0,
            Some(q) if q.contains("F16") || q.contains("f16") => 16.0,
            Some(q) if q.contains("F32") || q.contains("f32") => 32.0,
            _ => 4.5, // default Q4 assumption
        };
        let raw = (parameter_count as f64 * bits / 8.0) as u64;
        // 10% overhead for KV cache + runtime
        raw + raw / 10
    }
}

// ── Tiers ────────────────────────────────────────────────────────

/// A VRAM capacity tier — emergent from discovered hardware.
#[derive(Debug, Clone)]
pub struct Tier {
    /// Tier capacity in bytes (e.g. 8 GiB = 8_589_934_592).
    pub vram_bytes: u64,
    /// Display label (e.g. "8G").
    pub label: String,
    /// Endpoints of instances in this tier.
    pub instance_endpoints: Vec<String>,
}

// ── Lease ────────────────────────────────────────────────────────

/// Lease-on-demand reservation for a high-tier instance.
#[derive(Debug, Clone)]
pub struct Lease {
    pub instance_endpoint: String,
    pub model_name: String,
    pub granted_at: Instant,
    pub duration: std::time::Duration,
}

impl Lease {
    pub fn is_expired(&self) -> bool {
        self.granted_at.elapsed() > self.duration
    }

    /// Extend the lease with adaptive decay (+25%, cap at 5 min).
    pub fn extend(&mut self) {
        let new_dur = self.duration.mul_f64(1.25).min(std::time::Duration::from_secs(300));
        self.duration = new_dur;
        self.granted_at = Instant::now();
    }
}

// ── Routing Decision ─────────────────────────────────────────────

/// The result of a routing decision.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub target_endpoint: String,
    pub stone_name: String,
    pub model_name: String,
    pub tier_label: String,
    pub was_overflow: bool,
    pub lease_acquired: bool,
}

/// Why a routing decision failed.
#[derive(Debug, Clone)]
pub enum RoutingError {
    /// Model is unknown to the router.
    ModelNotFound(String),
    /// Model exists but no tier has enough VRAM.
    NoViableTier { model: String, vram_needed: u64 },
    /// All instances with capacity are fully busy.
    AllInstancesBusy { model: String },
    /// No healthy instances available at all.
    NoHealthyInstances,
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotFound(m) => write!(f, "model '{m}' not found in any instance"),
            Self::NoViableTier { model, vram_needed } => {
                write!(f, "no tier with enough VRAM for '{model}' (needs {vram_needed} bytes)")
            }
            Self::AllInstancesBusy { model } => write!(f, "all instances busy for '{model}'"),
            Self::NoHealthyInstances => write!(f, "no healthy Ollama instances"),
        }
    }
}

// ── Configuration ────────────────────────────────────────────────

/// Router configuration persisted as TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub stones: HashMap<String, StoneConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    #[serde(default = "default_true")]
    pub auto_pull: bool,
    #[serde(default)]
    pub delete_on_idle: bool,
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneConfig {
    /// Cap the VRAM budget below hardware maximum (MiB).
    pub vram_budget_mb: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            auto_pull: true,
            delete_on_idle: false,
            metrics_enabled: true,
        }
    }
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            features: FeatureConfig::default(),
            stones: HashMap::new(),
        }
    }
}

// ── Metrics ──────────────────────────────────────────────────────

/// Per-stone counters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoneMetrics {
    pub requests: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub errors: u64,
    /// Sum of response durations in nanoseconds (divide by requests for avg).
    pub total_duration_ns: u64,
}

/// Serializable metrics snapshot (persisted as JSON).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub tokens_in_total: u64,
    pub tokens_out_total: u64,
    pub errors_total: u64,
    pub per_stone: HashMap<String, StoneMetrics>,
    pub per_model: HashMap<String, u64>,
    pub started_at: Option<String>,
    pub snapshot_at: Option<String>,
}

// ── Ollama API Response Types ────────────────────────────────────

/// Response from `GET /api/tags`
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaTagsResponse {
    #[serde(default)]
    pub models: Vec<OllamaModelTag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelTag {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub families: Vec<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

/// Response from `GET /api/ps`
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaPsResponse {
    #[serde(default)]
    pub models: Vec<OllamaRunningModel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaRunningModel {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

/// Response from `POST /api/show`
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaShowResponse {
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
    #[serde(default)]
    pub model_info: Option<serde_json::Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl OllamaShowResponse {
    /// Extract `general.parameter_count` from `model_info`.
    pub fn parameter_count(&self) -> Option<u64> {
        self.model_info
            .as_ref()?
            .get("general.parameter_count")?
            .as_u64()
    }
}

/// Response from `GET /api/version`
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaVersionResponse {
    pub version: String,
}

/// Final NDJSON object from streaming inference (done: true).
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaInferenceFinal {
    #[serde(default)]
    pub done: bool,
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u64,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_count: u64,
    #[serde(default)]
    pub eval_duration: u64,
}

/// Pull progress event from `POST /api/pull` stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPullProgress {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}
