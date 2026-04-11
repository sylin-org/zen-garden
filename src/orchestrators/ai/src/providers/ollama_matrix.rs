//! Ollama capability matrix and layered-scoring selector
//! (ORCH-0030 §R2.2 commit 7).
//!
//! This module is the Ollama adapter's *private* view of what it
//! can serve right now. It holds three pieces of state:
//!
//! 1. **Per-instance state** — one [`InstanceEntry`] per live Ollama
//!    process, keyed by stone+endpoint. Carries the model list
//!    fetched from `/api/tags`, the loaded-model list from
//!    `/api/ps`, and the adapter's observed health.
//! 2. **Per-model metadata** — one [`ModelInfo`] per unique model
//!    name across all instances, with `/api/show` metadata:
//!    capability tags, parameter count, context length.
//! 3. **Layered scoring** — the [`OllamaSelector`] walks the matrix
//!    and picks the best `(model, instance)` pair for a request.
//!
//! # Scoring layers (harvested from the standalone Ollama orchestrator)
//!
//! - **Layer 0 — Availability (floor).** `+50` if available on any
//!   stone. `+10` per redundant stone (capped at `+30`). `+20` for
//!   warmth (loaded on ≥1 stone).
//! - **Layer 1 — Capability match.** Hard filter: only models whose
//!   Ollama tags include the requested primitive's tag
//!   (`completion` for chat, `embedding` for embed, `vision` for
//!   image.analyze). Models that don't declare the tag are removed,
//!   not deprioritized.
//! - **Layer 2 — Context window.** Per-capability cap. `synthesis`
//!   and `thinking` benefit most; `quick` gets 0.
//! - **Layer 3 — Parameter count (quality).** Per-capability cap
//!   and multiplier. Bigger models score higher for chat/tools/
//!   vision/synthesis; OCR has a deliberately low multiplier
//!   because a tuned 1B beats a generic 13B.
//! - **Layer 4 — Name affinity.** Small bonus when the model name
//!   contains the capability keyword (currently only OCR has a
//!   non-zero value).
//!
//! # Never recommend an unloadable model
//!
//! The selector's first step is to **union all instance model
//! lists** and keep only models that appear on at least one
//! healthy instance. A model from the catalog that isn't actually
//! installed anywhere is invisible to recommendation. This is the
//! load-bearing invariant that prevents the phantom-model bug we
//! saw in the parallel_smoke regression (commit 5 revert).
//!
//! # Pure logic, no I/O
//!
//! This module contains no network code, no async, no HTTP client.
//! The adapter (in `providers/ollama.rs`) constructs the matrix
//! from probe results and passes it to the selector. Unit tests
//! exercise the selector with synthetic matrices directly.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::domain::primitive::Primitive;

// ── Identity ─────────────────────────────────────────────────

/// Stable identifier for an Ollama instance: the full base URL.
/// Used as the primary key in the per-instance map.
pub type InstanceKey = String;

// ── Per-instance state ───────────────────────────────────────

/// A single Ollama instance's current state as observed by the
/// adapter. One entry per live process in the garden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceEntry {
    /// The instance's base URL (e.g. `http://stone-01:11434`).
    /// This is the primary key — the full HTTP endpoint including
    /// scheme and port.
    pub endpoint: String,

    /// Human-readable stone name (e.g. `stone-quartz-fen`).
    pub stone_name: String,

    /// Health as observed by the adapter. `Healthy` instances
    /// participate in selection; others are removed entirely.
    pub health: InstanceHealth,

    /// Models available on disk on this instance (from `/api/tags`).
    /// Short names only — no provider prefix.
    pub models_available: Vec<String>,

    /// Models currently loaded in VRAM on this instance (from
    /// `/api/ps`). Subset of `models_available`. Used by the
    /// scoring layer to award a warmth bonus.
    pub models_loaded: Vec<String>,

    /// Current in-flight request count as tracked by the adapter.
    /// Used by the tie-breaker when multiple candidates score
    /// identically — the less-loaded instance wins.
    pub queue_depth: u32,
}

impl InstanceEntry {
    pub fn new(endpoint: impl Into<String>, stone_name: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            stone_name: stone_name.into(),
            health: InstanceHealth::Profiling,
            models_available: Vec::new(),
            models_loaded: Vec::new(),
            queue_depth: 0,
        }
    }

    /// Whether this instance should be considered a candidate for
    /// routing. Mirrors the standalone orchestrator's definition:
    /// only `Healthy` is routable; `Profiling` and `Unhealthy` are
    /// removed from the candidate set entirely.
    pub fn is_routable(&self) -> bool {
        matches!(self.health, InstanceHealth::Healthy)
    }

    /// Whether the given model is currently loaded in VRAM on this
    /// instance. Used by the warmth bonus layer.
    pub fn has_model_loaded(&self, model: &str) -> bool {
        self.models_loaded.iter().any(|m| m == model)
    }

    /// Whether the given model is installed on disk on this
    /// instance. Used by the availability filter.
    pub fn has_model_available(&self, model: &str) -> bool {
        self.models_available.iter().any(|m| m == model)
    }
}

/// Instance health as observed by the adapter. Mirrors the
/// standalone Ollama orchestrator's three-state model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceHealth {
    /// Discovery reported this instance but the adapter has not
    /// yet completed its initial probe. Not routable.
    Profiling,
    /// Responding normally to probes. Routable.
    Healthy,
    /// Recently unreachable or erroring. Removed from the candidate
    /// set until a subsequent probe succeeds.
    Unhealthy,
}

// ── Per-model metadata ───────────────────────────────────────

/// Per-model metadata harvested from `/api/show` on the first
/// reachable instance that hosts the model. Identical across
/// instances for the same model name (model metadata is a property
/// of the model, not the instance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Short name as it appears in `/api/tags` (e.g. `llama3.1:8b`).
    pub name: String,

    /// Ollama-native capability tags: `"completion"`, `"embedding"`,
    /// `"vision"`, `"tools"`, `"thinking"`. The selector uses these
    /// to filter models by requested primitive.
    pub capabilities: Vec<String>,

    /// Parameter count in raw integer form (e.g. 7_000_000_000 for
    /// a 7B model). `None` if the model's metadata didn't report it.
    pub parameter_count: Option<u64>,

    /// Context window length in tokens, read from the
    /// architecture-specific `<arch>.context_length` key in
    /// `/api/show`'s `model_info`. Authoritative — Ollama reads it
    /// from the GGUF metadata at load time.
    pub context_length: Option<u64>,

    /// Disk size in bytes from `/api/tags`.
    pub size_bytes: u64,

    /// Maximum VRAM footprint ever observed for this model, in
    /// bytes. Populated from `/api/ps`'s `size_vram` field
    /// whenever the model is currently loaded on any instance
    /// (M4). `None` until the first observation — at which point
    /// the fit filter tightens from `size_bytes` (disk, a lower
    /// bound) to the measured value.
    ///
    /// The learning loop (M7) may revise this upward on a load
    /// failure — "model requires more system memory" responses
    /// bump this past the failing stone's total VRAM so the
    /// matrix rebuilt filters the model off that stone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_vram_bytes: Option<u64>,
}

impl ModelInfo {
    pub fn new(name: impl Into<String>, size_bytes: u64) -> Self {
        Self {
            name: name.into(),
            capabilities: Vec::new(),
            parameter_count: None,
            context_length: None,
            size_bytes,
            observed_vram_bytes: None,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_parameter_count(mut self, count: u64) -> Self {
        self.parameter_count = Some(count);
        self
    }

    pub fn with_context_length(mut self, ctx: u64) -> Self {
        self.context_length = Some(ctx);
        self
    }

    /// Parameters in billions, as a float. Used by the quality
    /// bonus layer. Returns 0.0 if `parameter_count` is None.
    pub fn parameter_billions(&self) -> f64 {
        self.parameter_count
            .map(|c| c as f64 / 1_000_000_000.0)
            .unwrap_or(0.0)
    }

    /// Whether this model declares the given Ollama capability tag.
    pub fn has_capability(&self, tag: &str) -> bool {
        self.capabilities.iter().any(|c| c == tag)
    }

    /// Best available estimate of the model's required VRAM in
    /// bytes for fit-filter purposes. Prefers the measured value
    /// from `/api/ps` when known; falls back to disk `size_bytes`
    /// as a conservative lower bound (loaded footprint is always
    /// at least as big as the GGUF file on disk).
    ///
    /// Returns 0 if neither signal is available — the caller
    /// interprets this as "unknown" and the fit filter treats
    /// unknown workloads permissively (matches old-orchestrator
    /// intent: don't block on absence of evidence).
    pub fn required_vram_bytes(&self) -> u64 {
        self.observed_vram_bytes.unwrap_or(self.size_bytes)
    }
}

// ── The matrix ───────────────────────────────────────────────

/// The Ollama adapter's complete view of what it can serve. Built
/// incrementally from probe results and handed to the
/// [`OllamaSelector`] for request-time decisions.
///
/// Ownership: the adapter holds one of these inside a `RwLock` and
/// mutates it from its probe task. Selection takes a read lock and
/// clones out only the fields it needs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaCapabilityMatrix {
    /// All known instances, keyed by endpoint URL.
    pub instances: HashMap<InstanceKey, InstanceEntry>,

    /// Metadata for every model observed across all instances,
    /// keyed by short name. An entry exists if any instance has
    /// the model installed; removal is wholesale on matrix rebuild.
    pub models: HashMap<String, ModelInfo>,
}

impl OllamaCapabilityMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the set of healthy instance keys. Selection filters
    /// candidates through this set; other stages never touch
    /// unhealthy instances.
    pub fn healthy_instances(&self) -> Vec<&InstanceEntry> {
        self.instances
            .values()
            .filter(|i| i.is_routable())
            .collect()
    }

    /// All model names that appear on at least one *healthy*
    /// instance. This is the **load-bearing anti-phantom filter**:
    /// if a model isn't installed anywhere routable, it cannot be
    /// recommended, period.
    pub fn loadable_models(&self) -> HashSet<&str> {
        let mut set = HashSet::new();
        for inst in self.instances.values() {
            if !inst.is_routable() {
                continue;
            }
            for m in &inst.models_available {
                set.insert(m.as_str());
            }
        }
        set
    }

    /// Union all instance model lists, then return only the models
    /// that pass the loadable filter AND declare the requested
    /// capability tag.
    pub fn eligible_models(&self, ollama_tag: &str) -> Vec<&ModelInfo> {
        let loadable = self.loadable_models();
        self.models
            .values()
            .filter(|m| loadable.contains(m.name.as_str()))
            .filter(|m| m.has_capability(ollama_tag))
            .collect()
    }

    /// Return every healthy instance that has the given model
    /// installed on disk. Used to resolve pinned model requests to
    /// an instance list.
    pub fn instances_with_model(&self, model: &str) -> Vec<&InstanceEntry> {
        self.instances
            .values()
            .filter(|i| i.is_routable() && i.has_model_available(model))
            .collect()
    }

    /// How many healthy instances have the given model *loaded in
    /// VRAM* right now. Used by the warmth bonus.
    pub fn warmth_count(&self, model: &str) -> usize {
        self.instances
            .values()
            .filter(|i| i.is_routable() && i.has_model_loaded(model))
            .count()
    }

    /// How many healthy instances have the given model on disk.
    /// Used by the redundancy bonus.
    pub fn availability_count(&self, model: &str) -> usize {
        self.instances
            .values()
            .filter(|i| i.is_routable() && i.has_model_available(model))
            .count()
    }

    /// Build the announcement the adapter publishes to the bus.
    /// Returns a sorted deduplicated list of Primitives the matrix
    /// can currently serve — one per Ollama capability tag that
    /// has at least one loadable model.
    pub fn supported_primitives(&self) -> Vec<Primitive> {
        let loadable = self.loadable_models();
        let mut tags: HashSet<&str> = HashSet::new();
        for m in self.models.values() {
            if !loadable.contains(m.name.as_str()) {
                continue;
            }
            for tag in &m.capabilities {
                tags.insert(tag.as_str());
            }
        }

        let mut out: Vec<Primitive> = Vec::new();
        // Completion → text.chat
        if tags.contains("completion") {
            out.push(Primitive::TextChat);
        }
        // Vision → image.analyze
        if tags.contains("vision") {
            out.push(Primitive::ImageAnalyze);
        }
        // Embedding → text.embed
        if tags.contains("embedding") {
            out.push(Primitive::TextEmbed);
        }
        out.sort_by_key(|p| p.dotted());
        out.dedup();
        out
    }
}

// ── Scoring inputs ───────────────────────────────────────────

/// User-facing capability label that maps onto Ollama's tag system
/// *plus* the orchestrator's richer scoring distinctions. One
/// primitive may have multiple capabilities: `text.chat` can be
/// asked for as `Chat` (quality-oriented) or `Quick` (speed-oriented).
/// The caller expresses this via a `selectors.capability` hint;
/// default for `text.chat` is `Chat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Fastest usable response. Autocomplete, extraction, one-liners.
    /// Maps to Ollama `completion`.
    Quick,
    /// Best conversational quality (the default for `text.chat`).
    /// Maps to Ollama `completion`.
    Chat,
    /// Long-context distillation and extraction.
    /// Maps to Ollama `completion`.
    Synthesis,
    /// Function calling / agent workflows. Maps to Ollama `tools`.
    Tools,
    /// Extended reasoning and analysis. Maps to Ollama `thinking`.
    Thinking,
    /// Semantic search and RAG. Maps to Ollama `embedding`.
    Embedding,
    /// Image understanding. Maps to Ollama `vision`.
    Vision,
    /// OCR and document reading. Maps to Ollama `vision` with a
    /// name-affinity bonus for purpose-built models.
    Ocr,
}

impl Capability {
    /// The Ollama capability tag this capability filters on.
    pub fn ollama_tag(self) -> &'static str {
        match self {
            Self::Quick | Self::Chat | Self::Synthesis => "completion",
            Self::Tools => "tools",
            Self::Thinking => "thinking",
            Self::Embedding => "embedding",
            Self::Vision | Self::Ocr => "vision",
        }
    }

    /// Default capability for a given primitive when the caller
    /// didn't provide a `selectors.capability` hint. The adapter
    /// uses this to resolve `recommended:chat`, `recommended:vision`,
    /// etc. — the bare primitive form.
    pub fn default_for(primitive: Primitive) -> Option<Self> {
        match primitive {
            Primitive::TextChat => Some(Self::Chat),
            Primitive::TextEmbed => Some(Self::Embedding),
            Primitive::ImageAnalyze => Some(Self::Vision),
            _ => None,
        }
    }

    /// Parse a `recommended:{capability}` selector moniker.
    /// Accepted forms: `"recommended:chat"`, `"recommended:vision"`,
    /// etc. Anything else returns `None`.
    pub fn parse_recommended(s: &str) -> Option<Self> {
        let rest = s.strip_prefix("recommended:")?;
        match rest {
            "quick" => Some(Self::Quick),
            "chat" | "completion" => Some(Self::Chat),
            "synthesis" => Some(Self::Synthesis),
            "tools" => Some(Self::Tools),
            "thinking" => Some(Self::Thinking),
            "embedding" => Some(Self::Embedding),
            "vision" => Some(Self::Vision),
            "ocr" => Some(Self::Ocr),
            _ => None,
        }
    }

    /// TPS bonus cap per capability. Speed matters for `Quick`,
    /// less for `Chat`, not at all for batch workloads like
    /// `Synthesis`.
    pub fn tps_bonus_cap(self) -> i64 {
        match self {
            Self::Quick => 200,
            Self::Chat => 50,
            Self::Tools | Self::Thinking => 30,
            Self::Synthesis | Self::Ocr => 0,
            _ => 0,
        }
    }

    /// Context window bonus cap. `Synthesis` benefits most; `Quick`
    /// not at all.
    pub fn context_bonus_cap(self) -> i64 {
        match self {
            Self::Synthesis => 500,
            Self::Thinking => 300,
            Self::Tools => 250,
            Self::Vision => 200,
            Self::Chat | Self::Ocr => 150,
            Self::Quick => 0,
            _ => 0,
        }
    }

    /// Quality bonus cap (applied to parameter count).
    pub fn quality_bonus_cap(self) -> i64 {
        match self {
            Self::Thinking => 500,
            Self::Tools | Self::Vision => 450,
            Self::Chat | Self::Synthesis => 400,
            Self::Ocr => 400,
            Self::Quick => 0,
            _ => 0,
        }
    }

    /// Quality multiplier — points per billion parameters.
    pub fn quality_multiplier(self) -> i64 {
        match self {
            Self::Thinking => 60,
            Self::Tools | Self::Vision => 50,
            Self::Chat | Self::Synthesis => 40,
            Self::Ocr => 15, // specialization > size
            _ => 0,
        }
    }

    /// Name-affinity bonus: a model whose name contains this
    /// keyword is purpose-built and earns a bonus. Currently only
    /// `Ocr` has a non-zero value.
    pub fn name_affinity(self) -> (Option<&'static str>, i64) {
        match self {
            Self::Ocr => (Some("ocr"), 300),
            _ => (None, 0),
        }
    }
}

// ── Scoring result ───────────────────────────────────────────

/// One candidate considered by the selector, with its score,
/// reasoning trace, and the instance it would route to.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub model: String,
    pub instance: String,
    pub stone_name: String,
    pub score: i64,
    pub reasoning: Vec<String>,
}

/// The selector's output: the winning candidate plus the alternates
/// in score order. Callers (the adapter's `onboard`) dispatch to
/// the winner and log the whole list for observability.
#[derive(Debug, Clone, Serialize)]
pub struct SelectionResult {
    pub winner: Candidate,
    pub alternates: Vec<Candidate>,
}

impl SelectionResult {
    /// Total number of candidates considered (winner + alternates).
    pub fn total(&self) -> usize {
        1 + self.alternates.len()
    }
}

/// Errors the selector can return.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SelectionError {
    #[error("no healthy Ollama instances")]
    NoHealthyInstances,

    #[error("no model on any instance declares capability `{capability}`")]
    NoEligibleModels { capability: &'static str },

    #[error(
        "pinned model `{model}` is not served by any healthy instance \
         (reason: {reason})"
    )]
    PinNotServable {
        model: String,
        reason: &'static str,
    },

    #[error("unknown primitive for Ollama: {0}")]
    UnsupportedPrimitive(Primitive),
}

// ── The selector ─────────────────────────────────────────────

/// Pure layered scoring over an [`OllamaCapabilityMatrix`]. No I/O,
/// no async, no network. Input is the matrix + the request's
/// effective capability + optional pin; output is a
/// [`SelectionResult`] or a [`SelectionError`].
pub struct OllamaSelector;

impl OllamaSelector {
    /// Select a model+instance pair for a recommended request.
    ///
    /// Flow:
    /// 1. Reject if no healthy instances.
    /// 2. Filter models by loadable + capability tag.
    /// 3. Score each candidate via layered scoring.
    /// 4. Pick the instance for the top model (prefer warm, then
    ///    lowest queue depth).
    /// 5. Return the winner plus alternates for observability.
    pub fn pick_recommended(
        matrix: &OllamaCapabilityMatrix,
        capability: Capability,
    ) -> Result<SelectionResult, SelectionError> {
        if matrix.healthy_instances().is_empty() {
            return Err(SelectionError::NoHealthyInstances);
        }

        let eligible = matrix.eligible_models(capability.ollama_tag());
        if eligible.is_empty() {
            return Err(SelectionError::NoEligibleModels {
                capability: capability.ollama_tag(),
            });
        }

        // Score every eligible model.
        let mut scored: Vec<(i64, Vec<String>, &ModelInfo)> = eligible
            .iter()
            .map(|m| {
                let (score, reasoning) = score_model(matrix, m, capability);
                (score, reasoning, *m)
            })
            .collect();

        // Sort by score descending, then name ascending for stability.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.name.cmp(&b.2.name)));

        // Build candidates by pairing each scored model with its
        // best instance (prefer warm, then least loaded).
        let mut candidates: Vec<Candidate> = Vec::new();
        for (score, reasoning, model) in scored {
            if let Some(inst) = pick_instance_for_model(matrix, &model.name) {
                candidates.push(Candidate {
                    model: model.name.clone(),
                    instance: inst.endpoint.clone(),
                    stone_name: inst.stone_name.clone(),
                    score,
                    reasoning,
                });
            }
        }

        if candidates.is_empty() {
            // This can happen only if a scored model's instances
            // all became unhealthy between filtering and scoring —
            // treat it as "no instances".
            return Err(SelectionError::NoHealthyInstances);
        }

        let winner = candidates.remove(0);
        Ok(SelectionResult {
            winner,
            alternates: candidates,
        })
    }

    /// Resolve a pinned model request to a specific instance.
    /// Returns `PinNotServable` if the model is not installed on
    /// any healthy instance.
    pub fn pick_pinned(
        matrix: &OllamaCapabilityMatrix,
        pinned_model: &str,
    ) -> Result<SelectionResult, SelectionError> {
        if matrix.healthy_instances().is_empty() {
            return Err(SelectionError::NoHealthyInstances);
        }

        let instances = matrix.instances_with_model(pinned_model);
        if instances.is_empty() {
            // Distinguish "model unknown entirely" from "model known
            // but not currently reachable".
            let known = matrix.models.contains_key(pinned_model);
            return Err(SelectionError::PinNotServable {
                model: pinned_model.to_string(),
                reason: if known {
                    "no healthy instance currently hosts this model"
                } else {
                    "model is not installed on any instance in the garden"
                },
            });
        }

        let winner_inst = pick_from_list(&instances);
        let winner = Candidate {
            model: pinned_model.to_string(),
            instance: winner_inst.endpoint.clone(),
            stone_name: winner_inst.stone_name.clone(),
            score: 0, // pinned bypasses scoring
            reasoning: vec![format!(
                "pinned model `{pinned_model}` served by {} / {}",
                winner_inst.stone_name, winner_inst.endpoint
            )],
        };
        let alternates: Vec<Candidate> = instances
            .iter()
            .filter(|i| i.endpoint != winner.instance)
            .map(|i| Candidate {
                model: pinned_model.to_string(),
                instance: i.endpoint.clone(),
                stone_name: i.stone_name.clone(),
                score: 0,
                reasoning: vec!["pinned alternate".to_string()],
            })
            .collect();
        Ok(SelectionResult { winner, alternates })
    }
}

/// Pick the best healthy instance for the given model, preferring
/// warm instances first, then lowest queue depth, then stable by
/// stone name.
fn pick_instance_for_model<'a>(
    matrix: &'a OllamaCapabilityMatrix,
    model: &str,
) -> Option<&'a InstanceEntry> {
    let mut candidates: Vec<&InstanceEntry> = matrix
        .instances
        .values()
        .filter(|i| i.is_routable() && i.has_model_available(model))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| {
        let warm_a = a.has_model_loaded(model);
        let warm_b = b.has_model_loaded(model);
        // Warm before cold
        warm_b
            .cmp(&warm_a)
            // Then least loaded
            .then(a.queue_depth.cmp(&b.queue_depth))
            // Then stable by stone name
            .then(a.stone_name.cmp(&b.stone_name))
    });
    Some(candidates[0])
}

fn pick_from_list<'a>(instances: &[&'a InstanceEntry]) -> &'a InstanceEntry {
    // Same ordering as pick_instance_for_model but given a slice.
    // Prefer least-loaded (pinned mode doesn't care about warmth
    // because the caller asked for a specific model regardless).
    let mut sorted: Vec<&InstanceEntry> = instances.iter().copied().collect();
    sorted.sort_by(|a, b| {
        a.queue_depth
            .cmp(&b.queue_depth)
            .then(a.stone_name.cmp(&b.stone_name))
    });
    sorted[0]
}

// ── Scoring ──────────────────────────────────────────────────

const SCORE_AVAILABLE: i64 = 50;
const SCORE_REDUNDANCY_PER_STONE: i64 = 10;
const SCORE_REDUNDANCY_CAP: i64 = 30;
const SCORE_WARMTH: i64 = 20;

fn score_model(
    matrix: &OllamaCapabilityMatrix,
    model: &ModelInfo,
    capability: Capability,
) -> (i64, Vec<String>) {
    let mut score: i64 = 0;
    let mut reasoning: Vec<String> = Vec::new();

    // ── Layer 0: Availability (floor) ─────────────────────
    let available_count = matrix.availability_count(&model.name);
    let warmth_count = matrix.warmth_count(&model.name);

    if available_count > 0 {
        score += SCORE_AVAILABLE;
        let extra = (available_count as i64 - 1) * SCORE_REDUNDANCY_PER_STONE;
        score += extra.min(SCORE_REDUNDANCY_CAP);
    }
    if warmth_count > 0 {
        score += SCORE_WARMTH;
    }

    if available_count > 1 {
        reasoning.push(format!("available on {available_count} stones"));
    } else if available_count == 1 {
        reasoning.push("available on 1 stone".to_string());
    }
    if warmth_count > 0 {
        reasoning.push(format!("warm on {warmth_count} stone(s)"));
    }

    // ── Layer 2: Context window bonus ─────────────────────
    // (Layer 1 — capability filter — is applied before this function
    // by `eligible_models`.)
    let ctx_cap = capability.context_bonus_cap();
    if ctx_cap > 0 {
        if let Some(ctx) = model.context_length {
            let bonus = ((ctx as i64) / 1000).min(ctx_cap);
            score += bonus;
            if ctx >= 32_000 {
                reasoning.push(format!("{}K context window", ctx / 1000));
            }
        }
    }

    // ── Layer 3: Quality bonus ────────────────────────────
    let q_cap = capability.quality_bonus_cap();
    let q_mul = capability.quality_multiplier();
    if q_cap > 0 && q_mul > 0 {
        let params_b = model.parameter_billions();
        if params_b > 0.0 {
            let bonus = ((params_b * q_mul as f64) as i64).min(q_cap);
            score += bonus;
            if params_b >= 3.0 {
                reasoning.push(format!("{:.0}B parameters", params_b));
            }
        }
    }

    // ── Layer 4: Name affinity ────────────────────────────
    let (keyword, affinity) = capability.name_affinity();
    if let (Some(keyword), true) = (keyword, affinity > 0) {
        if model.name.to_lowercase().contains(keyword) {
            score += affinity;
            reasoning.push(format!("purpose-built {keyword} model"));
        }
    }

    (score, reasoning)
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_instance(
        endpoint: &str,
        stone: &str,
        available: &[&str],
        loaded: &[&str],
    ) -> InstanceEntry {
        InstanceEntry {
            endpoint: endpoint.into(),
            stone_name: stone.into(),
            health: InstanceHealth::Healthy,
            models_available: available.iter().map(|s| s.to_string()).collect(),
            models_loaded: loaded.iter().map(|s| s.to_string()).collect(),
            queue_depth: 0,
        }
    }

    fn model_info(
        name: &str,
        caps: &[&str],
        params: Option<u64>,
        ctx: Option<u64>,
    ) -> ModelInfo {
        ModelInfo {
            name: name.into(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            parameter_count: params,
            context_length: ctx,
            size_bytes: 4_000_000_000,
            observed_vram_bytes: None,
        }
    }

    fn chat_model(name: &str, params_b: f64) -> ModelInfo {
        model_info(
            name,
            &["completion"],
            Some((params_b * 1_000_000_000.0) as u64),
            Some(8192),
        )
    }

    fn vision_model(name: &str, params_b: f64) -> ModelInfo {
        model_info(
            name,
            &["completion", "vision"],
            Some((params_b * 1_000_000_000.0) as u64),
            Some(8192),
        )
    }

    fn embed_model(name: &str) -> ModelInfo {
        model_info(name, &["embedding"], Some(137_000_000), Some(512))
    }

    fn populated_matrix() -> OllamaCapabilityMatrix {
        let mut m = OllamaCapabilityMatrix::new();
        // Three stones: large + two smalls
        m.instances.insert(
            "http://stone-01:11434".into(),
            healthy_instance(
                "http://stone-01:11434",
                "stone-01",
                &["qwen2.5:24b", "llava:13b", "nomic-embed-text"],
                &["qwen2.5:24b"],
            ),
        );
        m.instances.insert(
            "http://stone-02:11434".into(),
            healthy_instance(
                "http://stone-02:11434",
                "stone-02",
                &["qwen2.5:8b", "nomic-embed-text"],
                &["qwen2.5:8b"],
            ),
        );
        m.instances.insert(
            "http://stone-03:11434".into(),
            healthy_instance(
                "http://stone-03:11434",
                "stone-03",
                &["qwen2.5:4b", "nomic-embed-text"],
                &[],
            ),
        );
        m.models
            .insert("qwen2.5:24b".into(), chat_model("qwen2.5:24b", 24.0));
        m.models
            .insert("qwen2.5:8b".into(), chat_model("qwen2.5:8b", 8.0));
        m.models
            .insert("qwen2.5:4b".into(), chat_model("qwen2.5:4b", 4.0));
        m.models
            .insert("llava:13b".into(), vision_model("llava:13b", 13.0));
        m.models
            .insert("nomic-embed-text".into(), embed_model("nomic-embed-text"));
        m
    }

    // ── Matrix queries ─────────────────────────────────────────

    #[test]
    fn loadable_models_excludes_unhealthy_instances() {
        let mut m = populated_matrix();
        // Mark stone-02 unhealthy — its unique model qwen2.5:8b
        // should drop out of loadable. qwen2.5:24b remains (on
        // stone-01) and qwen2.5:4b remains (on stone-03).
        m.instances
            .get_mut("http://stone-02:11434")
            .unwrap()
            .health = InstanceHealth::Unhealthy;
        let loadable = m.loadable_models();
        assert!(loadable.contains("qwen2.5:24b"));
        assert!(loadable.contains("qwen2.5:4b"));
        assert!(!loadable.contains("qwen2.5:8b"));
    }

    #[test]
    fn eligible_models_filters_by_ollama_tag() {
        let m = populated_matrix();
        let chat = m.eligible_models("completion");
        assert!(chat.iter().any(|m| m.name == "qwen2.5:24b"));
        assert!(chat.iter().any(|m| m.name == "qwen2.5:8b"));
        assert!(chat.iter().any(|m| m.name == "llava:13b")); // also completion
        assert!(!chat.iter().any(|m| m.name == "nomic-embed-text"));

        let embed = m.eligible_models("embedding");
        assert_eq!(embed.len(), 1);
        assert_eq!(embed[0].name, "nomic-embed-text");

        let vision = m.eligible_models("vision");
        assert_eq!(vision.len(), 1);
        assert_eq!(vision[0].name, "llava:13b");
    }

    #[test]
    fn warmth_count_and_availability_count() {
        let m = populated_matrix();
        assert_eq!(m.availability_count("qwen2.5:24b"), 1);
        assert_eq!(m.warmth_count("qwen2.5:24b"), 1);
        assert_eq!(m.availability_count("nomic-embed-text"), 3);
        assert_eq!(m.warmth_count("nomic-embed-text"), 0);
    }

    #[test]
    fn supported_primitives_union_across_instances() {
        let m = populated_matrix();
        let prims = m.supported_primitives();
        assert!(prims.contains(&Primitive::TextChat));
        assert!(prims.contains(&Primitive::TextEmbed));
        assert!(prims.contains(&Primitive::ImageAnalyze));
    }

    #[test]
    fn supported_primitives_empty_when_all_unhealthy() {
        let mut m = populated_matrix();
        for inst in m.instances.values_mut() {
            inst.health = InstanceHealth::Unhealthy;
        }
        assert!(m.supported_primitives().is_empty());
    }

    // ── Capability mapping ─────────────────────────────────────

    #[test]
    fn capability_parse_recommended_accepts_standard_forms() {
        assert_eq!(
            Capability::parse_recommended("recommended:chat"),
            Some(Capability::Chat)
        );
        assert_eq!(
            Capability::parse_recommended("recommended:vision"),
            Some(Capability::Vision)
        );
        assert_eq!(
            Capability::parse_recommended("recommended:embedding"),
            Some(Capability::Embedding)
        );
        assert_eq!(
            Capability::parse_recommended("recommended:completion"),
            Some(Capability::Chat)
        );
        assert_eq!(
            Capability::parse_recommended("recommended:unknown"),
            None
        );
        assert_eq!(Capability::parse_recommended("llama3.1:8b"), None);
    }

    #[test]
    fn capability_default_for_primitive() {
        assert_eq!(
            Capability::default_for(Primitive::TextChat),
            Some(Capability::Chat)
        );
        assert_eq!(
            Capability::default_for(Primitive::TextEmbed),
            Some(Capability::Embedding)
        );
        assert_eq!(
            Capability::default_for(Primitive::ImageAnalyze),
            Some(Capability::Vision)
        );
    }

    // ── Selector: recommended ──────────────────────────────────

    #[test]
    fn selector_picks_largest_model_for_chat() {
        let m = populated_matrix();
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Chat).expect("must select");
        // qwen2.5:24b is largest and warm → should win
        assert_eq!(result.winner.model, "qwen2.5:24b");
        assert_eq!(result.winner.stone_name, "stone-01");
        assert!(result.alternates.len() >= 1);
    }

    #[test]
    fn selector_picks_only_vision_model_for_vision() {
        let m = populated_matrix();
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Vision).expect("must select");
        assert_eq!(result.winner.model, "llava:13b");
    }

    #[test]
    fn selector_picks_only_embed_model_for_embedding() {
        let m = populated_matrix();
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Embedding).expect("must select");
        assert_eq!(result.winner.model, "nomic-embed-text");
    }

    #[test]
    fn selector_empty_matrix_returns_no_healthy() {
        let m = OllamaCapabilityMatrix::new();
        let err = OllamaSelector::pick_recommended(&m, Capability::Chat).unwrap_err();
        assert!(matches!(err, SelectionError::NoHealthyInstances));
    }

    #[test]
    fn selector_all_unhealthy_returns_no_healthy() {
        let mut m = populated_matrix();
        for inst in m.instances.values_mut() {
            inst.health = InstanceHealth::Unhealthy;
        }
        let err = OllamaSelector::pick_recommended(&m, Capability::Chat).unwrap_err();
        assert!(matches!(err, SelectionError::NoHealthyInstances));
    }

    #[test]
    fn selector_no_eligible_models_returns_specific_error() {
        // A matrix with only embedding models — asking for chat.
        let mut m = OllamaCapabilityMatrix::new();
        m.instances.insert(
            "http://stone-01:11434".into(),
            healthy_instance(
                "http://stone-01:11434",
                "stone-01",
                &["nomic-embed-text"],
                &[],
            ),
        );
        m.models
            .insert("nomic-embed-text".into(), embed_model("nomic-embed-text"));
        let err = OllamaSelector::pick_recommended(&m, Capability::Chat).unwrap_err();
        assert!(matches!(
            err,
            SelectionError::NoEligibleModels {
                capability: "completion"
            }
        ));
    }

    #[test]
    fn selector_picks_warm_over_cold_when_scores_tie() {
        // Two identical models on two stones, one warm, one cold.
        let mut m = OllamaCapabilityMatrix::new();
        m.instances.insert(
            "http://stone-01:11434".into(),
            healthy_instance(
                "http://stone-01:11434",
                "stone-01",
                &["llama3.1:8b"],
                &["llama3.1:8b"], // warm
            ),
        );
        m.instances.insert(
            "http://stone-02:11434".into(),
            healthy_instance(
                "http://stone-02:11434",
                "stone-02",
                &["llama3.1:8b"],
                &[], // cold
            ),
        );
        m.models.insert("llama3.1:8b".into(), chat_model("llama3.1:8b", 8.0));
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Chat).expect("select");
        // Warmth wins the tie
        assert_eq!(result.winner.stone_name, "stone-01");
    }

    #[test]
    fn selector_picks_least_loaded_when_warmth_ties() {
        // Both stones cold, stone-02 has lower queue depth.
        let mut m = OllamaCapabilityMatrix::new();
        let mut a = healthy_instance(
            "http://stone-01:11434",
            "stone-01",
            &["llama3.1:8b"],
            &[],
        );
        a.queue_depth = 5;
        let mut b = healthy_instance(
            "http://stone-02:11434",
            "stone-02",
            &["llama3.1:8b"],
            &[],
        );
        b.queue_depth = 1;
        m.instances.insert(a.endpoint.clone(), a);
        m.instances.insert(b.endpoint.clone(), b);
        m.models.insert("llama3.1:8b".into(), chat_model("llama3.1:8b", 8.0));
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Chat).expect("select");
        assert_eq!(result.winner.stone_name, "stone-02");
    }

    // ── Selector: pinned ───────────────────────────────────────

    #[test]
    fn selector_pin_picks_the_specific_model() {
        let m = populated_matrix();
        // Pin the 8B chat model explicitly — it should route to
        // stone-02 regardless of what scoring would say.
        let result =
            OllamaSelector::pick_pinned(&m, "qwen2.5:8b").expect("pin resolves");
        assert_eq!(result.winner.model, "qwen2.5:8b");
        assert_eq!(result.winner.stone_name, "stone-02");
    }

    #[test]
    fn selector_pin_unknown_model_returns_pin_not_servable() {
        let m = populated_matrix();
        let err = OllamaSelector::pick_pinned(&m, "nonexistent:model").unwrap_err();
        assert!(matches!(
            err,
            SelectionError::PinNotServable { ref model, .. }
                if model == "nonexistent:model"
        ));
    }

    #[test]
    fn selector_pin_known_model_but_no_healthy_instance_returns_pin_not_servable() {
        let mut m = populated_matrix();
        // llava:13b is only on stone-01; mark stone-01 unhealthy.
        m.instances
            .get_mut("http://stone-01:11434")
            .unwrap()
            .health = InstanceHealth::Unhealthy;
        let err = OllamaSelector::pick_pinned(&m, "llava:13b").unwrap_err();
        match err {
            SelectionError::PinNotServable { model, reason } => {
                assert_eq!(model, "llava:13b");
                // The model IS known to the matrix; the reason
                // should mention "no healthy instance", not "not
                // installed".
                assert!(reason.contains("no healthy"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn selector_pin_empty_matrix_returns_no_healthy() {
        let m = OllamaCapabilityMatrix::new();
        let err = OllamaSelector::pick_pinned(&m, "llama3.1:8b").unwrap_err();
        assert!(matches!(err, SelectionError::NoHealthyInstances));
    }

    // ── Scoring layer invariants ───────────────────────────────

    #[test]
    fn scoring_awards_redundancy_bonus_capped() {
        // Build a matrix with the same model on 10 stones and
        // verify the redundancy bonus caps at SCORE_REDUNDANCY_CAP.
        let mut m = OllamaCapabilityMatrix::new();
        for i in 0..10 {
            let endpoint = format!("http://stone-{i:02}:11434");
            let stone = format!("stone-{i:02}");
            m.instances.insert(
                endpoint.clone(),
                healthy_instance(&endpoint, &stone, &["llama3.1:8b"], &[]),
            );
        }
        m.models
            .insert("llama3.1:8b".into(), chat_model("llama3.1:8b", 8.0));
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Chat).expect("select");
        // Base 50 + redundancy cap 30 + quality (8B × 40 = 320) +
        // context bonus (8k/1000 = 8, capped at 150 → 8).
        // Total: 50 + 30 + 320 + 8 = 408
        assert_eq!(result.winner.score, 408);
    }

    #[test]
    fn scoring_quality_multiplier_differs_per_capability() {
        // 8B model: chat score adds 8×40 = 320 (capped at 400).
        // thinking score would add 8×60 = 480 (capped at 500).
        let mut m = OllamaCapabilityMatrix::new();
        m.instances.insert(
            "http://stone-01:11434".into(),
            healthy_instance(
                "http://stone-01:11434",
                "stone-01",
                &["test:8b"],
                &[],
            ),
        );
        m.models.insert(
            "test:8b".into(),
            model_info("test:8b", &["completion", "thinking"], Some(8_000_000_000), Some(8192)),
        );
        let chat = OllamaSelector::pick_recommended(&m, Capability::Chat).unwrap();
        let thinking =
            OllamaSelector::pick_recommended(&m, Capability::Thinking).unwrap();
        // Thinking should score higher because of the multiplier
        assert!(thinking.winner.score > chat.winner.score);
    }

    #[test]
    fn scoring_reasoning_includes_availability_and_warmth() {
        let m = populated_matrix();
        let result = OllamaSelector::pick_recommended(&m, Capability::Chat).unwrap();
        let reasoning_blob = result.winner.reasoning.join(" | ");
        assert!(
            reasoning_blob.contains("available") || reasoning_blob.contains("warm"),
            "reasoning should mention availability or warmth: {}",
            reasoning_blob
        );
    }

    #[test]
    fn scoring_anti_phantom_filter_rejects_uninstalled_models() {
        // A matrix where `qwen2.5:24b` is in the models map but
        // no instance has it installed. The selector must not
        // recommend it.
        let mut m = OllamaCapabilityMatrix::new();
        m.instances.insert(
            "http://stone-01:11434".into(),
            healthy_instance(
                "http://stone-01:11434",
                "stone-01",
                &["qwen2.5:8b"], // only 8b installed
                &[],
            ),
        );
        m.models
            .insert("qwen2.5:24b".into(), chat_model("qwen2.5:24b", 24.0));
        m.models
            .insert("qwen2.5:8b".into(), chat_model("qwen2.5:8b", 8.0));
        let result =
            OllamaSelector::pick_recommended(&m, Capability::Chat).expect("select");
        // 24b scores higher on paper but is unreachable → 8b wins.
        assert_eq!(result.winner.model, "qwen2.5:8b");
        assert!(!result.alternates.iter().any(|c| c.model == "qwen2.5:24b"));
    }
}
