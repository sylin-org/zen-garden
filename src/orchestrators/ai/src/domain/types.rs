//! Core domain types for the AI orchestrator.
//!
//! These types are the shared vocabulary between all layers: domain, catalog,
//! tasks, and API. They carry domain meaning, not architectural role (code
//! standard §3).

use std::time::Instant;

use serde::{Deserialize, Serialize};

// ── Capability ──────────────────────────────────────────────────────

/// AI capability — what a service instance can do.
///
/// Unified from the Ollama orchestrator's `fitness::Capability` and
/// `demand::RequestCapability`. See ORCH-0013 migration table for the
/// mapping from old variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // ── Text/LLM (from Ollama orchestrator) ─────────────────────────

    /// Raw token generation — the fitness/benchmark concept.
    /// Measures generation speed independent of chat formatting.
    Generate,

    /// Conversational request — the demand/routing concept.
    /// Maps to `Generate` benchmark thresholds for fitness scoring.
    Chat,

    /// Text to vector embedding.
    Embed,

    /// Image + text to text (multimodal vision).
    Vision,

    /// Structured tool/function calling.
    Tools,

    /// Sustained long-generation under KV cache pressure.
    Think,

    // ── Generation (new) ────────────────────────────────────────────

    /// Text to image.
    Imagine,

    /// Image + instruction to image (inpaint, edit).
    Edit,

    /// Text to video.
    Render,

    // ── Audio (new) ─────────────────────────────────────────────────

    /// Audio to text (speech-to-text).
    Transcribe,

    /// Text to audio (text-to-speech).
    Speak,

    // ── Search/Retrieval (new) ──────────────────────────────────────

    /// Query + documents to scored/ranked documents.
    Rerank,

    // ── Language (new) ──────────────────────────────────────────────

    /// Text + target language to translated text.
    Translate,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generate => write!(f, "generate"),
            Self::Chat => write!(f, "chat"),
            Self::Embed => write!(f, "embed"),
            Self::Vision => write!(f, "vision"),
            Self::Tools => write!(f, "tools"),
            Self::Think => write!(f, "think"),
            Self::Imagine => write!(f, "imagine"),
            Self::Edit => write!(f, "edit"),
            Self::Render => write!(f, "render"),
            Self::Transcribe => write!(f, "transcribe"),
            Self::Speak => write!(f, "speak"),
            Self::Rerank => write!(f, "rerank"),
            Self::Translate => write!(f, "translate"),
        }
    }
}

impl Capability {
    /// All known capability variants (for iteration).
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

    /// The fitness capability used for benchmark threshold evaluation.
    ///
    /// `Chat` maps to `Generate` thresholds — a chat request is benchmarked
    /// as token generation. All other capabilities map to themselves.
    pub fn fitness_capability(self) -> Self {
        match self {
            Self::Chat => Self::Generate,
            other => other,
        }
    }
}

// ── Offering Kind ───────────────────────────────────────────────────

/// Offering type discriminator — enum, not String (code standard §8).
///
/// Every AI service type the orchestrator can manage. Cloud providers are
/// distinct kinds so that routing, priority, and health check intervals
/// can be configured per-provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferingKind {
    // Local/garden offerings
    Ollama,
    ComfyUi,
    WhisperCpp,
    Speaches,
    OpenedaiSpeech,
    Infinity,
    LibreTranslate,

    // Cloud providers
    HuggingFace,
    OpenAi,
    Anthropic,
    StabilityAi,
    ElevenLabs,
    Cohere,
    Deepgram,
    Google,
}

impl std::fmt::Display for OfferingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::ComfyUi => write!(f, "comfyui"),
            Self::WhisperCpp => write!(f, "whispercpp"),
            Self::Speaches => write!(f, "speaches"),
            Self::OpenedaiSpeech => write!(f, "openedai-speech"),
            Self::Infinity => write!(f, "infinity"),
            Self::LibreTranslate => write!(f, "libretranslate"),
            Self::HuggingFace => write!(f, "huggingface"),
            Self::OpenAi => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::StabilityAi => write!(f, "stability-ai"),
            Self::ElevenLabs => write!(f, "elevenlabs"),
            Self::Cohere => write!(f, "cohere"),
            Self::Deepgram => write!(f, "deepgram"),
            Self::Google => write!(f, "google"),
        }
    }
}

impl OfferingKind {
    /// Whether this offering kind is a cloud provider.
    pub fn is_cloud(&self) -> bool {
        matches!(
            self,
            Self::HuggingFace
                | Self::OpenAi
                | Self::Anthropic

                | Self::StabilityAi
                | Self::ElevenLabs
                | Self::Cohere
                | Self::Deepgram
                | Self::Google
        )
    }
}

// ── Service Instance ────────────────────────────────────────────────

/// A discovered AI service instance on a stone.
///
/// Generalized from the Ollama orchestrator's `OllamaInstance`. Field naming
/// follows code standards: struct nesting for namespaces (§1), no type-in-name
/// (§2), value objects for identity (§7).
#[derive(Debug, Clone, Serialize)]
pub struct ServiceInstance {
    /// Stone identity.
    pub stone: Stone,
    /// HTTP endpoint for this instance (e.g., "http://192.168.1.10:11434").
    pub endpoint: String,
    /// Offering type.
    pub kind: OfferingKind,

    /// GPU hardware.
    pub gpu: Gpu,
    /// VRAM state.
    pub vram: Vram,

    /// Health state.
    pub health: InstanceHealth,
    /// Model/resource names available on this instance.
    pub models_available: Vec<String>,
    /// Models currently loaded in VRAM.
    pub models_loaded: Vec<LoadedModel>,
    /// Capabilities this instance can serve.
    pub capabilities: Vec<Capability>,
    /// Number of inflight requests.
    pub queue_depth: u32,
    /// Last successful probe time.
    #[serde(skip)]
    pub last_seen: Instant,

    /// Offering-specific metadata (opaque to routing).
    pub metadata: serde_json::Value,

    /// Routing priority. Higher = preferred.
    /// 0 = default, +10 = pinned, -10 = cloud.
    pub priority: i32,
}

/// Stone identity (code standard §7 — value object, not flat primitives).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stone {
    /// Permanent cryptographic/install identity.
    pub id: String,
    /// User-assigned display name.
    pub name: String,
}

/// GPU identity (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gpu {
    /// GPU model name (e.g., "NVIDIA GeForce RTX 4090").
    pub name: Option<String>,
    /// Compute type.
    pub compute: ComputeType,
}

/// VRAM state (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vram {
    /// Total VRAM in bytes.
    pub total_bytes: u64,
    /// VRAM budget (total minus system reservation).
    pub budget_bytes: u64,
    /// Real-time free VRAM (from probe). `None` if not reported.
    pub free_bytes: Option<u64>,
}

/// Compute type — GPU or CPU inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeType {
    Gpu,
    Cpu,
}

/// Instance health state.
///
/// `Unhealthy` carries diagnostic context (timestamp + reason) for routing
/// cooldown logic and dashboard display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum InstanceHealth {
    /// Initial profiling in progress.
    Profiling,
    /// Healthy and routable.
    Healthy,
    /// Failed health probes — not routable.
    Unhealthy {
        /// When the instance was marked unhealthy (for cooldown logic).
        #[serde(skip, default = "std::time::Instant::now")]
        since: std::time::Instant,
        /// Human-readable reason (for dashboard and tracing).
        reason: String,
    },
}

impl InstanceHealth {
    /// Whether this instance is routable.
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// A model currently loaded in VRAM on an instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    /// Model name.
    pub name: String,
    /// VRAM consumed by this model (bytes).
    pub vram_bytes: u64,
    /// Auto-unload TTL (Ollama's `keep_alive`). `None` if no expiry.
    pub expires_at: Option<String>,
}

// ── Benchmark Sample ────────────────────────────────────────────────

/// One measurement from a benchmark prompt/input.
///
/// Defined in domain (not catalog) because the fitness module aggregates
/// these into verdicts. The catalog re-exports this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    /// Index within the test suite (0-based).
    pub prompt_index: usize,
    /// Cold start latency in milliseconds.
    pub cold_start_ms: u64,
    /// Tokens per second (for text generation capabilities).
    pub tokens_per_second: Option<f64>,
    /// Total wall-clock duration in milliseconds.
    pub total_duration_ms: u64,
    /// For tool-calling: ratio of correct tool selections (0.0-1.0).
    pub valid_ratio: Option<f64>,
    /// Error message if this sample failed.
    pub error: Option<String>,
}

// ── Cross-Offering VRAM ─────────────────────────────────────────────

/// Aggregate VRAM state for one stone across all offerings.
///
/// Assembled by the tasks layer from individual `ServiceInstance` VRAM data.
/// Fed to the domain routing function for cross-offering VRAM awareness.
#[derive(Debug, Clone, Serialize)]
pub struct StoneVramBudget {
    /// Stone identity.
    pub stone: Stone,
    /// Total GPU VRAM on this stone.
    pub total_bytes: u64,
    /// Combined VRAM used by all offerings.
    pub used_bytes: u64,
    /// Available VRAM (total - used).
    pub free_bytes: u64,
    /// Per-offering breakdown.
    pub per_offering: Vec<OfferingVramUsage>,
}

/// VRAM usage by a single offering on a stone.
#[derive(Debug, Clone, Serialize)]
pub struct OfferingVramUsage {
    /// Offering type.
    pub kind: OfferingKind,
    /// VRAM consumed by this offering's loaded models.
    pub used_bytes: u64,
    /// Number of models loaded.
    pub model_count: usize,
}

// ── Routing ─────────────────────────────────────────────────────────

/// Result of the routing algorithm — which instance to forward to.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Target endpoint URL.
    pub endpoint: String,
    /// Stone identity of the selected instance.
    pub stone: Stone,
    /// Resolved model name (after moniker resolution).
    pub model: String,
    /// Offering type of the selected instance.
    pub kind: OfferingKind,
    /// VRAM tier label.
    pub tier: String,
    /// Whether this was an overflow selection (no ideal candidate).
    pub was_overflow: bool,
    /// Whether a lease was acquired for this request.
    pub lease_acquired: bool,
}

/// Routing error — why no instance could be selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    /// No instance has the requested model.
    ModelNotFound(String),
    /// Model exists but all instances are fitness-blocked.
    ModelBlocked(String),
    /// All instances with the model are at max queue depth.
    AllInstancesBusy { model: String },
    /// No healthy instances are available.
    NoHealthyInstances,
    /// No instance supports the requested capability.
    CapabilityNotAvailable { capability: Capability },
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotFound(m) => write!(f, "model '{m}' not found on any instance"),
            Self::ModelBlocked(m) => write!(f, "model '{m}' blocked on all instances (fitness)"),
            Self::AllInstancesBusy { model } => {
                write!(f, "all instances with '{model}' at max queue depth")
            }
            Self::NoHealthyInstances => write!(f, "no healthy instances"),
            Self::CapabilityNotAvailable { capability } => {
                write!(f, "no instance supports capability '{capability}'")
            }
        }
    }
}

impl std::error::Error for RoutingError {}

// ── Fitness ─────────────────────────────────────────────────────────

/// Fitness verdict — how well an instance performs a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Excellent performance. Score: 100.
    Fast,
    /// Acceptable but slow. Score: 50.
    Degraded,
    /// Poor performance; router tries others first. Score: 10.
    Vetoed,
    /// Hard failure; excluded from routing (except pinned). Score: 0.
    Blocked,
}

impl Verdict {
    /// Numeric score for sorting (higher = better).
    pub fn score(self) -> u32 {
        match self {
            Self::Fast => 100,
            Self::Degraded => 50,
            Self::Vetoed => 10,
            Self::Blocked => 0,
        }
    }

    /// Whether this verdict excludes the instance from routing.
    pub fn is_blocked(self) -> bool {
        self == Self::Blocked
    }
}

// ── Tier ────────────────────────────────────────────────────────────

/// VRAM capacity tier — instances grouped by VRAM budget (rounded to GiB).
#[derive(Debug, Clone, Serialize)]
pub struct Tier {
    /// Tier label (e.g., "24G").
    pub label: String,
    /// VRAM budget in bytes (lower bound for this tier).
    pub vram_bytes: u64,
    /// Instance endpoints in this tier.
    pub endpoints: Vec<String>,
}

// ── Model Info ──────────────────────────────────────────────────────

/// Model metadata — generalized from Ollama's `ModelInfo`.
///
/// Offering adapters populate this from their enumeration responses.
/// Fields that don't apply to an offering (e.g., `parameter_count` for
/// a translation model) are `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    /// Number of parameters (e.g., 8 billion for llama3:8b).
    pub parameter_count: Option<u64>,
    /// Parameter size string (e.g., "8B").
    pub parameter_size: Option<String>,
    /// Quantization level (e.g., "Q4_K_M").
    pub quantization_level: Option<String>,
    /// Model family (e.g., "llama").
    pub family: Option<String>,
    /// Model families/tags.
    pub families: Vec<String>,
    /// Capability tags (e.g., ["vision", "thinking", "tools"]).
    pub capabilities: Vec<String>,
    /// Model format (e.g., "gguf", "safetensors").
    pub format: Option<String>,
    /// Size on disk in bytes.
    pub size_disk: u64,
    /// Authoritative VRAM usage in bytes — sourced from runtime probe
    /// only when the model is loaded. `None` means never observed.
    pub vram_bytes: Option<u64>,
    /// Context window in tokens.
    pub context_length: Option<u64>,
}

// ── Config ──────────────────────────────────────────────────────────

/// Router configuration (persisted as TOML).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouterConfig {
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub stones: std::collections::HashMap<String, StoneConfig>,
}

/// Feature toggles and global settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    #[serde(default)]
    pub auto_pull_mode: AutoPullMode,
    #[serde(default)]
    pub delete_on_idle: bool,
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,
    /// User-pinned model overrides per capability (e.g., "chat" -> "qwen3.5:9b").
    #[serde(default)]
    pub pins: std::collections::HashMap<String, String>,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            auto_pull_mode: AutoPullMode::default(),
            delete_on_idle: false,
            metrics_enabled: true,
            pins: std::collections::HashMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Per-stone overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneConfig {
    /// Cap the VRAM budget below hardware maximum (MiB).
    pub vram_budget_mb: Option<u64>,
}

/// Auto-pull mode — controls model sync and on-demand pull behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoPullMode {
    /// No automatic model management. Unknown model -> 404.
    Off,
    /// Replicate models across stones in the same tier. Unknown -> 404.
    #[default]
    Sync,
    /// Sync + pull unknown models on demand.
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

// ── Placement ───────────────────────────────────────────────────────

/// Ideal model-to-stone assignment computed from demand distribution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlacementPlan {
    /// model_name -> list of target endpoints that should hold this model.
    pub assignments: std::collections::HashMap<String, Vec<String>>,
    /// When this plan was last computed (ISO-8601).
    pub computed_at: Option<String>,
    /// True if this plan matched the previous computation (hysteresis).
    pub stable: bool,
}

// ── Metrics ─────────────────────────────────────────────────────────

/// Per-stone cumulative counters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoneMetrics {
    pub requests: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub errors: u64,
    /// Sum of response durations in nanoseconds.
    pub total_duration_ns: u64,
    /// Sum of eval (generation-only) durations in nanoseconds.
    pub eval_duration_ns: u64,
}

/// Serializable metrics snapshot (persisted as JSON).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub tokens_in_total: u64,
    pub tokens_out_total: u64,
    pub errors_total: u64,
    pub per_stone: std::collections::HashMap<String, StoneMetrics>,
    pub per_model: std::collections::HashMap<String, u64>,
    pub started_at: Option<String>,
    pub snapshot_at: Option<String>,
}

/// Event sent from the proxy to the metrics processor.
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

// ── Jobs ────────────────────────────────────────────────────────────

/// Background job kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ModelPull,
    ModelDelete,
    ModelSync,
    InstanceProfile,
    OnDemandPull,
    Benchmark,
}

/// Background job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// Tracked background job.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestratorJob {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub detail: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}
