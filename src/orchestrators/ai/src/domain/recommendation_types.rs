//! Value objects for the recommendation engine.
//!
//! The engine itself lives in [`crate::services::recommendation`]; these
//! types are declared in the domain layer because they are referenced
//! by the directory snapshot and by the operator HTTP endpoints.
//!
//! ## Capability profiles
//!
//! Callers ask for models via use-case labels — `chat`,
//! `quickchat`, `think`, `vision`, `tools`, `embedding`, … — not
//! per-primitive. Each label is a [`CapabilityProfile`]: a small
//! declarative bundle that describes which primitive it serves,
//! which provider-side `capability_tag` makes a model eligible,
//! and how to score eligible models. Multiple profiles can target
//! the same primitive (e.g. `chat`, `quickchat`, `think` all
//! produce text but reward very different models).
//!
//! Adding a new capability is one entry in the registry — no other
//! code touches.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::ids::ModelFqn;
use crate::domain::primitive::Primitive;
use crate::domain::provider::PerformanceVerdict;

// ── Capability profile ────────────────────────────────────────

/// A user-facing capability label and the rules used to rank
/// candidate models for it.
#[derive(Debug, Clone)]
pub struct CapabilityProfile {
    /// The label callers send via `recommended:<name>`.
    pub name: &'static str,
    /// Which primitive a winning model serves. Used by the
    /// contextualizer to wire the resolved model to the right
    /// dispatch path.
    pub primitive: Primitive,
    /// A model is **eligible** when its `capability_tags` contains
    /// any of these strings. Most profiles need exactly one tag
    /// (`"completion"`, `"vision"`, `"embedding"`, …). Tools and
    /// thinking each require their own tag in addition to the
    /// generic completion tag — discovery sets both for models that
    /// support them.
    pub required_tags: &'static [&'static str],
    /// Per-capability scoring knobs. Tunable per profile because
    /// the right model for `quickchat` (small + fast) is the wrong
    /// one for `think` (big + long-context).
    pub weights: ScoringWeights,
    /// Optional parameter-count constraints. Models outside the
    /// range are filtered before scoring. Used to keep
    /// `quickchat` from picking a 70B model and `think` from
    /// picking a 1B one.
    pub size_floor_billions: Option<f64>,
    pub size_ceiling_billions: Option<f64>,
    /// Optional name-affinity bonus: models whose short name
    /// contains this substring (case-insensitive) are
    /// purpose-built for the capability. `ocr` benefits the most.
    pub name_affinity: Option<NameAffinity>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScoringWeights {
    /// Base score awarded to every eligible model.
    pub eligibility_base: i64,
    /// Bonus per billion parameters, capped at `quality_cap`.
    pub quality_per_billion: i64,
    pub quality_cap: i64,
    /// Bonus per 1k tokens of context window, capped at
    /// `context_cap`.
    pub context_per_1k_tokens: i64,
    pub context_cap: i64,
    /// Verdict weights from the (optional) performance hint layer.
    pub verdict_fast: i64,
    pub verdict_degraded: i64,
    pub verdict_vetoed: i64,
    /// Pin override — when an operator pin matches an eligible
    /// model, this bonus is added to make sure it ranks first.
    pub pin_bonus: i64,
}

impl ScoringWeights {
    /// Sensible defaults for chat-shaped capabilities. Profiles
    /// override individual fields.
    pub const fn chat_defaults() -> Self {
        Self {
            eligibility_base: 100,
            quality_per_billion: 40,
            quality_cap: 400,
            context_per_1k_tokens: 1,
            context_cap: 150,
            verdict_fast: 300,
            verdict_degraded: 50,
            verdict_vetoed: -500,
            pin_bonus: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NameAffinity {
    pub keyword: &'static str,
    pub bonus: i64,
}

// ── Profile registry ──────────────────────────────────────────

/// Static registry of every capability the orchestrator
/// recognises. New capabilities are added by extending this
/// table — no engine or contextualizer changes required.
pub struct CapabilityProfileRegistry {
    profiles: Vec<CapabilityProfile>,
}

impl CapabilityProfileRegistry {
    pub fn build() -> Self {
        Self {
            profiles: default_profiles(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&CapabilityProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityProfile> {
        self.profiles.iter()
    }

    /// The default capability label for a primitive. Used when a
    /// caller dispatches a bare primitive without a model selector
    /// — the contextualizer asks the engine for the recommended
    /// model under that primitive's default capability.
    pub fn default_for_primitive(&self, primitive: Primitive) -> Option<&CapabilityProfile> {
        // First profile in declaration order whose primitive
        // matches is the default. The table is ordered with
        // defaults first (chat before quickchat, vision before ocr,
        // …).
        self.profiles.iter().find(|p| p.primitive == primitive)
    }
}

impl Default for CapabilityProfileRegistry {
    fn default() -> Self {
        Self::build()
    }
}

/// The declarative profile table. Order matters: the first profile
/// matching a primitive is its default.
fn default_profiles() -> Vec<CapabilityProfile> {
    use Primitive::*;
    vec![
        // ── Text chat family ──────────────────────────────────
        CapabilityProfile {
            name: "chat",
            primitive: TextChat,
            required_tags: &["completion"],
            weights: ScoringWeights {
                quality_per_billion: 40,
                quality_cap: 400,
                context_per_1k_tokens: 1,
                context_cap: 150,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "quickchat",
            primitive: TextChat,
            required_tags: &["completion"],
            weights: ScoringWeights {
                // Quickchat values speed and small size — quality
                // bonus is muted, context bonus is zero, larger
                // models lose ground.
                quality_per_billion: -20,
                quality_cap: 0,
                context_per_1k_tokens: 0,
                context_cap: 0,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            // Cap at ~5B params: keeps the recommender from
            // picking a 70B model when the user wants snappy.
            size_ceiling_billions: Some(5.0),
            name_affinity: None,
        },
        CapabilityProfile {
            name: "think",
            primitive: TextChat,
            // `thinking` is the strict tag; falls back to
            // `completion` if no thinking-tagged model exists, but
            // we filter strictly for now.
            required_tags: &["thinking"],
            weights: ScoringWeights {
                quality_per_billion: 60,
                quality_cap: 500,
                context_per_1k_tokens: 3,
                context_cap: 300,
                ..ScoringWeights::chat_defaults()
            },
            // Reasoning models below ~6B are usually demos.
            size_floor_billions: Some(6.0),
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "tools",
            primitive: TextChat,
            required_tags: &["tools"],
            weights: ScoringWeights {
                quality_per_billion: 50,
                quality_cap: 450,
                context_per_1k_tokens: 2,
                context_cap: 250,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "synthesis",
            primitive: TextChat,
            required_tags: &["completion"],
            weights: ScoringWeights {
                quality_per_billion: 40,
                quality_cap: 400,
                // Synthesis is all about long context.
                context_per_1k_tokens: 5,
                context_cap: 500,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: Some(3.0),
            size_ceiling_billions: None,
            name_affinity: None,
        },
        // ── Translate ────────────────────────────────────────
        CapabilityProfile {
            name: "translate",
            primitive: TextTranslate,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        // ── Embed ────────────────────────────────────────────
        CapabilityProfile {
            name: "embed",
            primitive: TextEmbed,
            required_tags: &["embedding"],
            weights: ScoringWeights {
                // Embedding models reward context length more than
                // raw size — `nomic-embed-text` (8K ctx) beats
                // `all-minilm` (256) for most workloads even at a
                // smaller param count.
                quality_per_billion: 5,
                quality_cap: 50,
                context_per_1k_tokens: 4,
                context_cap: 300,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: Some(NameAffinity {
                keyword: "embed",
                bonus: 100,
            }),
        },
        // ── Rerank ───────────────────────────────────────────
        CapabilityProfile {
            name: "rerank",
            primitive: TextRerank,
            required_tags: &["rerank"],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: Some(NameAffinity {
                keyword: "rerank",
                bonus: 100,
            }),
        },
        // ── Image analyze (vision) + OCR ─────────────────────
        CapabilityProfile {
            name: "vision",
            primitive: ImageAnalyze,
            required_tags: &["vision"],
            weights: ScoringWeights {
                quality_per_billion: 50,
                quality_cap: 450,
                context_per_1k_tokens: 2,
                context_cap: 200,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: Some(NameAffinity {
                keyword: "vl",
                bonus: 50,
            }),
        },
        CapabilityProfile {
            name: "ocr",
            primitive: ImageAnalyze,
            required_tags: &["vision"],
            weights: ScoringWeights {
                // OCR rewards specialization, not size — a tuned
                // 1B model beats a generic 13B.
                quality_per_billion: 15,
                quality_cap: 200,
                context_per_1k_tokens: 1,
                context_cap: 150,
                ..ScoringWeights::chat_defaults()
            },
            size_floor_billions: None,
            size_ceiling_billions: None,
            // Purpose-built OCR models dominate.
            name_affinity: Some(NameAffinity {
                keyword: "ocr",
                bonus: 300,
            }),
        },
        // ── Image generate / edit / upscale ──────────────────
        CapabilityProfile {
            name: "image_generate",
            primitive: ImageGenerate,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "image_edit",
            primitive: ImageEdit,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "image_upscale",
            primitive: ImageUpscale,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        // ── Audio ────────────────────────────────────────────
        CapabilityProfile {
            name: "speech",
            primitive: AudioGenerate,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
        CapabilityProfile {
            name: "transcribe",
            primitive: AudioTranscribe,
            required_tags: &[],
            weights: ScoringWeights::chat_defaults(),
            size_floor_billions: None,
            size_ceiling_billions: None,
            name_affinity: None,
        },
    ]
}

// ── Recommendation cache + result types ───────────────────────

/// The full cache produced by the
/// [`crate::services::recommendation::RecommendationEngine`].
/// Keyed by capability label, not by primitive — multiple
/// capabilities may share a primitive.
#[derive(Debug, Clone)]
pub struct RecommendationCache {
    pub version: u64,
    pub built_at: DateTime<Utc>,
    pub per_capability: HashMap<String, RankedRecommendations>,
}

/// Per-capability ranked recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedRecommendations {
    pub capability: String,
    pub primitive: String,
    pub selected: Option<ModelFqn>,
    pub candidates: Vec<Recommendation>,
    pub reasoning: Vec<String>,
}

/// A single model's recommendation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub model: ModelFqn,
    pub rank: u32,
    pub score: i64,
    pub pinned: bool,
    pub verdict: Option<PerformanceVerdict>,
    pub reasoning: Vec<String>,
}

/// Operator-scoped pin for a capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    /// Capability label this pin applies to
    /// (e.g. `"chat"`, `"quickchat"`).
    pub capability: String,
    pub model: ModelFqn,
    pub pinned_at: DateTime<Utc>,
    pub pinned_by: Option<String>,
    pub note: Option<String>,
}
