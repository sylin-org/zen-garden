//! Core domain types for the AI Orchestrator.
//!
//! Pure data structures — no I/O, no async. Generalized from the Ollama
//! orchestrator's domain types. Every type here replaces its Ollama-specific
//! counterpart (`OllamaInstance` → `ServiceInstance`, etc.).

use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Offering Kind ───────────────────────────────────────────────

/// Offering type discriminator — enum, not String (code standard §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferingKind {
    Ollama,
    ComfyUi,
    Speaches,
    OpenedaiSpeech,
    Infinity,
    LibreTranslate,
    HuggingFace,
    // Cloud providers
    OpenAi,
    Anthropic,
    StabilityAi,
    ElevenLabs,
    Cohere,
    Deepgram,
    Google,
}

impl OfferingKind {
    /// Well-known proxy port for this offering type.
    /// Returns `None` for cloud providers (no proxy port).
    pub fn proxy_port(&self) -> Option<u16> {
        match self {
            Self::Ollama => Some(21434),
            Self::ComfyUi => Some(21435),
            Self::Speaches => Some(21436),         // whisper.cpp
            Self::OpenedaiSpeech => Some(21437),
            Self::Infinity => Some(21438),
            Self::LibreTranslate => Some(21439),
            Self::HuggingFace => None,             // cloud — no proxy
            Self::OpenAi => None,
            Self::Anthropic => None,
            Self::StabilityAi => None,
            Self::ElevenLabs => None,
            Self::Cohere => None,
            Self::Deepgram => None,
            Self::Google => None,
        }
    }

    /// Short string identifier (matches topology filter names).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::ComfyUi => "comfyui",
            Self::Speaches => "speaches",
            Self::OpenedaiSpeech => "openedai-speech",
            Self::Infinity => "infinity",
            Self::LibreTranslate => "libretranslate",
            Self::HuggingFace => "huggingface",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::StabilityAi => "stability-ai",
            Self::ElevenLabs => "elevenlabs",
            Self::Cohere => "cohere",
            Self::Deepgram => "deepgram",
            Self::Google => "google",
        }
    }

    /// Well-known service port for the offering's native API.
    /// Returns `None` for cloud providers (no local port).
    pub fn default_service_port(&self) -> Option<u16> {
        match self {
            Self::Ollama => Some(11434),
            Self::ComfyUi => Some(8188),
            Self::Speaches => Some(8000),
            Self::OpenedaiSpeech => Some(8000),
            Self::Infinity => Some(7997),
            Self::LibreTranslate => Some(5000),
            Self::HuggingFace => None,
            Self::OpenAi => None,
            Self::Anthropic => None,
            Self::StabilityAi => None,
            Self::ElevenLabs => None,
            Self::Cohere => None,
            Self::Deepgram => None,
            Self::Google => None,
        }
    }

    /// Parse an offering name from topology into an `OfferingKind`.
    ///
    /// Matches the `offering` field in `TopologyServiceEntry` against
    /// known AI offering names. Returns `None` for unrecognized names.
    pub fn from_topology_name(name: &str) -> Option<Self> {
        match name {
            "ollama" | "ollama-cpu" => Some(Self::Ollama),
            "comfyui" => Some(Self::ComfyUi),
            "speaches" => Some(Self::Speaches),
            "openedai-speech" => Some(Self::OpenedaiSpeech),
            "infinity" => Some(Self::Infinity),
            "libretranslate" => Some(Self::LibreTranslate),
            "huggingface" => Some(Self::HuggingFace),
            _ => None,
        }
    }

    /// All local (non-cloud) offering type names used for topology filtering.
    pub const LOCAL_OFFERING_NAMES: &[&str] = &[
        "ollama",
        "ollama-cpu",
        "comfyui",
        "speaches",
        "openedai-speech",
        "infinity",
        "libretranslate",
    ];

    /// Whether this offering type is a cloud provider (priority -10 by default).
    pub fn is_cloud(&self) -> bool {
        matches!(
            self,
            Self::OpenAi
                | Self::Anthropic
                | Self::StabilityAi
                | Self::ElevenLabs
                | Self::Cohere
                | Self::Deepgram
                | Self::Google
                | Self::HuggingFace
        )
    }
}

impl std::fmt::Display for OfferingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Capability ──────────────────────────────────────────────────

/// Unified capability enum. Merges the Ollama orchestrator's separate
/// `fitness::Capability` and `demand::RequestCapability` enums.
///
/// See ORCH-0013 migration table for the merge rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // Text/LLM — existing Ollama orchestrator variants (names preserved)
    Generate, // raw token generation (fitness/benchmark concept)
    Chat,     // conversational request (demand/routing concept)
    Embed,    // text → vector (renamed from demand::Embedding)
    Vision,   // image + text → text
    Tools,    // structured tool-calling
    Think,    // sustained long-generation (renamed from demand::Thinking)

    // Generation (new)
    Imagine,    // text → image
    Edit,       // image + instruction → image
    Render,     // text → video

    // Audio (new)
    Transcribe, // audio → text
    Speak,      // text → audio

    // Search/Retrieval (new)
    Rerank, // query + docs → scored docs

    // Language (new)
    Translate, // text + target → text
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Chat => "chat",
            Self::Embed => "embed",
            Self::Vision => "vision",
            Self::Tools => "tools",
            Self::Think => "think",
            Self::Imagine => "imagine",
            Self::Edit => "edit",
            Self::Render => "render",
            Self::Transcribe => "transcribe",
            Self::Speak => "speak",
            Self::Rerank => "rerank",
            Self::Translate => "translate",
        }
    }

    /// All known capabilities.
    pub const ALL: &[Self] = &[
        Self::Generate,
        Self::Chat,
        Self::Embed,
        Self::Vision,
        Self::Tools,
        Self::Think,
        Self::Imagine,
        Self::Edit,
        Self::Render,
        Self::Transcribe,
        Self::Speak,
        Self::Rerank,
        Self::Translate,
    ];
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Service Instance (generalized from OllamaInstance) ──────────

/// Whether an instance uses GPU or CPU for inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComputeType {
    Gpu,
    Cpu,
}

impl std::fmt::Display for ComputeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu => write!(f, "gpu"),
            Self::Cpu => write!(f, "cpu"),
        }
    }
}

/// Stone identity (code standard §7 — value object, not flat strings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stone {
    pub id: String,
    pub name: String,
}

/// GPU identity (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone)]
pub struct Gpu {
    pub name: Option<String>,
    pub compute: ComputeType,
}

/// VRAM state (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone)]
pub struct Vram {
    pub total_bytes: u64,
    pub budget_bytes: u64,
    /// Real-time VRAM free bytes (ComfyUI provides this; Ollama does not).
    pub free_bytes: Option<u64>,
}

/// A discovered AI service instance and its hardware profile.
///
/// Replaces `OllamaInstance`. Field naming follows code standards:
/// struct nesting for namespaces (§1), no type-in-name (§2).
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    // Identity
    pub stone: Stone,
    pub endpoint: String,
    pub kind: OfferingKind,

    // Hardware
    pub gpu: Gpu,
    pub vram: Vram,

    // Service state
    pub health: InstanceHealth,
    pub models_available: Vec<String>,
    pub models_loaded: Vec<LoadedModel>,
    pub capabilities: Vec<Capability>,
    pub queue_depth: u32,
    pub last_seen: Instant,

    // Offering-specific metadata (opaque to routing)
    pub metadata: serde_json::Value,

    // Priority (0 = default, -10 = cloud, +10 = pinned)
    pub priority: i32,
}

impl ServiceInstance {
    /// Whether this instance participates in routing.
    pub fn is_routable(&self) -> bool {
        self.health.is_routable()
    }
}

/// Instance health as observed by the orchestrator.
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
    /// Exact VRAM consumption (bytes).
    pub size_vram: u64,
    /// When the service will auto-unload (ISO-8601, offering-specific).
    pub expires_at: Option<String>,
}

// ── Model Info ───────────────────────────────────────────────────

/// Model metadata gathered from offering enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub parameter_count: Option<u64>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub family: Option<String>,
    pub families: Vec<String>,
    pub capabilities: Vec<String>,
    pub format: Option<String>,
    pub size_disk: u64,
    pub vram_bytes: Option<u64>,
    pub context_length: Option<u64>,
}

// ── Tiers ────────────────────────────────────────────────────────

/// A VRAM capacity tier — emergent from discovered hardware.
#[derive(Debug, Clone)]
pub struct Tier {
    pub vram_bytes: u64,
    pub label: String,
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

    pub fn extend(&mut self) {
        let new_dur = self
            .duration
            .mul_f64(1.25)
            .min(std::time::Duration::from_secs(300));
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
    pub offering_kind: OfferingKind,
    pub was_overflow: bool,
    pub lease_acquired: bool,
}

/// Why a routing decision failed.
#[derive(Debug, Clone)]
pub enum RoutingError {
    ModelNotFound(String),
    ModelBlocked(String),
    AllInstancesBusy { model: String },
    NoHealthyInstances,
    /// No offering serves this capability.
    CapabilityUnavailable(Capability),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotFound(m) => write!(f, "model '{m}' not found in any instance"),
            Self::ModelBlocked(m) => {
                write!(f, "model '{m}' is blocked on all available stones (benchmark errors)")
            }
            Self::AllInstancesBusy { model } => write!(f, "all instances busy for '{model}'"),
            Self::NoHealthyInstances => write!(f, "no healthy instances"),
            Self::CapabilityUnavailable(c) => write!(f, "no offering serves capability '{c}'"),
        }
    }
}

// ── Auto-Pull Mode ──────────────────────────────────────────────

/// Three-way auto-pull policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoPullMode {
    Off,
    #[default]
    Sync,
    OnDemand,
}

impl std::fmt::Display for AutoPullMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Sync => write!(f, "sync"),
            Self::OnDemand => write!(f, "on_demand"),
        }
    }
}

// ── Orchestrator Jobs ───────────────────────────────────────────

/// What kind of work a job performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ModelPull { model: String, targets: Vec<String> },
    ModelDelete { model: String, targets: Vec<String> },
    ModelSync { model: String, targets: Vec<String> },
    ResourceSync { resource: String, offering: String, targets: Vec<String> },
    InstanceProfile { endpoint: String, stone_name: String },
    OnDemandPull { model: String },
    Benchmark { scope: String, stones: Vec<String> },
}

impl JobKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ModelPull { .. } => "pull",
            Self::ModelDelete { .. } => "delete",
            Self::ModelSync { .. } => "sync",
            Self::ResourceSync { .. } => "resource-sync",
            Self::InstanceProfile { .. } => "profile",
            Self::OnDemandPull { .. } => "on-demand",
            Self::Benchmark { .. } => "benchmark",
        }
    }

    pub fn subject(&self) -> &str {
        match self {
            Self::ModelPull { model, .. } => model,
            Self::ModelDelete { model, .. } => model,
            Self::ModelSync { model, .. } => model,
            Self::ResourceSync { resource, .. } => resource,
            Self::InstanceProfile { stone_name, .. } => stone_name,
            Self::OnDemandPull { model, .. } => model,
            Self::Benchmark { scope, .. } => scope,
        }
    }
}

/// Current status of a job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// A tracked orchestrator job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorJob {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

// ── Configuration ────────────────────────────────────────────────

/// Orchestrator configuration persisted as TOML.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub stones: HashMap<String, StoneConfig>,
    /// Per-offering proxy enable/disable overrides.
    #[serde(default)]
    pub proxies: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    #[serde(default)]
    pub auto_pull_mode: AutoPullMode,
    #[serde(default)]
    pub delete_on_idle: bool,
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,
    /// User-pinned model overrides per capability.
    #[serde(default)]
    pub pins: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneConfig {
    pub vram_budget_mb: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            auto_pull_mode: AutoPullMode::default(),
            delete_on_idle: false,
            metrics_enabled: true,
            pins: HashMap::new(),
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
    pub total_duration_ns: u64,
    pub eval_duration_ns: u64,
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

// ── Metric Events ───────────────────────────────────────────────

/// Metric event sent from the proxy to the metrics processing task.
#[derive(Debug, Clone)]
pub enum MetricEvent {
    Request {
        stone: String,
        model: String,
        capability: Capability,
        tokens_in: u64,
        tokens_out: u64,
        duration_ns: u64,
        eval_duration_ns: u64,
    },
    Error {
        stone: String,
        model: Option<String>,
        status_code: Option<u16>,
        reason: Option<String>,
    },
}

// ── Placement ────────────────────────────────────────────────────

/// Demand-weighted placement plan: ideal model→stone assignment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub assignments: HashMap<String, Vec<String>>,
    pub computed_at: Option<String>,
    pub stable: bool,
}

// ── Cross-Offering VRAM Accounting ──────────────────────────────

/// Aggregate VRAM state for one stone across all offerings.
#[derive(Debug, Clone)]
pub struct StoneVramBudget {
    pub stone_id: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub per_offering: Vec<OfferingVramUsage>,
}

#[derive(Debug, Clone)]
pub struct OfferingVramUsage {
    pub kind: OfferingKind,
    pub used_bytes: u64,
    pub model_count: usize,
}
