//! Capability-aware model recommendations.
//!
//! Pure scoring logic — no async, no I/O. Given model metadata, instance
//! state, and benchmark fitness data, produces a ranked list of models for
//! a requested capability.
//!
//! Generalized from ollama-orchestrator domain/recommendation.rs — uses
//! unified Capability enum and ServiceInstance instead of Ollama-specific types.
//!
//! ## Layered Scoring
//!
//! - **Layer 0 (Availability)**: binary presence + redundancy bonus.
//! - **Layer 1 (Fitness)**: best-stone-only verdict, throughput, cold start.
//! - **Layer 2 (Context)**: context window bonus (cap varies by capability).
//! - **Layer 3 (Quality)**: model size bonus from parameter count.

use std::collections::HashMap;

use serde::Serialize;

use super::fitness::GpuMatrix;
use super::types::{Capability, ModelInfo, ServiceInstance, Verdict};

// ── Public types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationResponse {
    pub capability: String,
    pub selected: Option<String>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub model: String,
    pub rank: u32,
    pub score: i64,
    pub pinned: bool,
    pub verdict: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub context_length: Option<u64>,
    pub reasoning: Vec<String>,
}

const MAX_RECOMMENDATIONS: usize = 5;

// ── Capability mapping ──────────────────────────────────────────

/// Map a user-facing capability string to the fitness `Capability` enum.
fn fitness_capability(cap: &str) -> Option<Capability> {
    match cap {
        "tools" => Some(Capability::Tools),
        "thinking" => Some(Capability::Think),
        "quick" | "chat" | "completion" | "synthesis" => Some(Capability::Generate),
        "embedding" | "embed" => Some(Capability::Embed),
        "vision" | "ocr" => Some(Capability::Vision),
        "imagine" => Some(Capability::Imagine),
        "transcribe" => Some(Capability::Transcribe),
        "speak" => Some(Capability::Speak),
        "rerank" => Some(Capability::Rerank),
        "translate" => Some(Capability::Translate),
        _ => None,
    }
}

/// Map a user-facing capability to a model capability tag for filtering.
///
/// Models declare their capabilities as string tags. This function maps
/// the user's request to the tag set used for filtering eligible models.
fn capability_filter_tag(cap: &str) -> &str {
    match cap {
        "quick" | "chat" | "completion" | "synthesis" | "tools" | "thinking" => "completion",
        "embedding" | "embed" => "embedding",
        "vision" | "ocr" => "vision",
        "imagine" | "edit" | "render" => "imagine",
        "transcribe" => "transcribe",
        "speak" => "speak",
        "rerank" => "rerank",
        "translate" => "translate",
        _ => cap,
    }
}

// ── Scoring constants ───────────────────────────────────────────

const SCORE_AVAILABLE: i64 = 50;
const SCORE_REDUNDANCY_PER_STONE: i64 = 10;
const SCORE_REDUNDANCY_CAP: i64 = 30;
const SCORE_LOADED: i64 = 20;
const SCORE_VERDICT_FAST: i64 = 300;
const SCORE_VERDICT_DEGRADED: i64 = 150;
const SCORE_VERDICT_VETOED: i64 = 30;
const SCORE_VERDICT_BLOCKED: i64 = -500;
const COLD_PENALTY_CAP: i64 = 50;

fn tps_bonus_cap(cap: &str) -> i64 {
    match cap {
        "quick" => 200,
        "chat" | "completion" => 50,
        "tools" | "thinking" => 30,
        _ => 0,
    }
}

fn context_bonus_cap(cap: &str) -> i64 {
    match cap {
        "synthesis" => 500,
        "thinking" => 300,
        "tools" => 250,
        "vision" => 200,
        "chat" | "completion" | "ocr" => 150,
        _ => 0,
    }
}

fn quality_bonus_cap(cap: &str) -> i64 {
    match cap {
        "thinking" => 500,
        "tools" | "vision" => 450,
        "chat" | "completion" | "synthesis" | "ocr" => 400,
        _ => 0,
    }
}

fn quality_multiplier(cap: &str) -> i64 {
    match cap {
        "thinking" => 60,
        "tools" | "vision" => 50,
        "chat" | "completion" | "synthesis" => 40,
        "ocr" => 15,
        _ => 0,
    }
}

fn name_affinity_bonus(cap: &str) -> i64 {
    match cap {
        "ocr" => 300,
        _ => 0,
    }
}

// ── Core recommendation function ────────────────────────────────

/// Produce ranked recommendations for the given capability.
pub fn recommend(
    capability: &str,
    models: &HashMap<String, ModelInfo>,
    instances: &HashMap<String, ServiceInstance>,
    gpu_matrix: &GpuMatrix,
    pin: Option<&str>,
) -> RecommendationResponse {
    let fitness_cap = fitness_capability(capability);
    let cap_filter = match capability {
        "tools" => "tools",
        "thinking" => "thinking",
        _ => capability_filter_tag(capability),
    };

    // Filter: only models declaring the requested capability.
    let eligible: Vec<&ModelInfo> = models
        .values()
        .filter(|m| m.capabilities.iter().any(|c| c == cap_filter))
        .collect();

    // Build instance lookup: model_name → Vec<(stone_name, endpoint, loaded)>
    let mut model_stones: HashMap<&str, Vec<(&str, &str, bool)>> = HashMap::new();
    for inst in instances.values() {
        if !inst.health.is_healthy() {
            continue;
        }
        for model_name in &inst.models_available {
            let loaded = inst.models_loaded.iter().any(|l| l.name == *model_name);
            model_stones
                .entry(model_name.as_str())
                .or_default()
                .push((&inst.stone.name, &inst.endpoint, loaded));
        }
    }

    let mut scored: Vec<Recommendation> = eligible
        .iter()
        .map(|m| score_model(m, capability, fitness_cap, &model_stones, gpu_matrix))
        .collect();

    scored.sort_by(|a, b| b.score.cmp(&a.score).then(a.model.cmp(&b.model)));

    // Apply pin.
    if let Some(pinned_name) = pin {
        if let Some(pos) = scored.iter().position(|r| r.model == pinned_name) {
            let mut pinned = scored.remove(pos);
            pinned.pinned = true;
            scored.insert(0, pinned);
        }
    }

    scored.truncate(MAX_RECOMMENDATIONS);

    for (i, rec) in scored.iter_mut().enumerate() {
        rec.rank = (i + 1) as u32;
    }

    let selected = scored.first().map(|r| r.model.clone());

    RecommendationResponse {
        capability: capability.to_string(),
        selected,
        recommendations: scored,
    }
}

// ── Per-model scoring ───────────────────────────────────────────

fn score_model(
    model: &ModelInfo,
    capability: &str,
    fitness_cap: Option<Capability>,
    model_stones: &HashMap<&str, Vec<(&str, &str, bool)>>,
    gpu_matrix: &GpuMatrix,
) -> Recommendation {
    let mut score: i64 = 0;
    let mut reasoning: Vec<String> = Vec::new();
    let mut best_verdict: Option<Verdict> = None;

    // Layer 0: Availability
    let stones = model_stones.get(model.name.as_str());
    let available_count = stones.map(|s| s.len()).unwrap_or(0);
    let loaded_count = stones
        .map(|s| s.iter().filter(|(_, _, loaded)| *loaded).count())
        .unwrap_or(0);

    if available_count > 0 {
        score += SCORE_AVAILABLE;
        let extra = (available_count as i64 - 1) * SCORE_REDUNDANCY_PER_STONE;
        score += extra.min(SCORE_REDUNDANCY_CAP);
    }
    if loaded_count > 0 {
        score += SCORE_LOADED;
    }
    if available_count > 1 {
        reasoning.push(format!("available on {} stones", available_count));
    } else if available_count == 1 {
        reasoning.push("available on 1 stone".to_string());
    }

    // Layer 1: Fitness (best stone only)
    let mut has_fitness = false;
    let mut best_fitness_score: i64 = i64::MIN;
    let mut best_fitness_tps: f64 = 0.0;
    let mut best_fitness_cold: u64 = 0;

    if let (Some(stones), Some(cap)) = (stones, fitness_cap) {
        for &(_stone_name, endpoint, _loaded) in stones {
            let entry = gpu_matrix
                .entries
                .iter()
                .find(|e| e.model == model.name && e.capability == cap && e.endpoint == endpoint)
                .or_else(|| {
                    if matches!(cap, Capability::Tools | Capability::Think) {
                        gpu_matrix.entries.iter().find(|e| {
                            e.model == model.name
                                && e.capability == Capability::Generate
                                && e.endpoint == endpoint
                        })
                    } else {
                        None
                    }
                });

            if let Some(e) = entry {
                has_fitness = true;
                let verdict_score = match e.verdict {
                    Verdict::Fast => SCORE_VERDICT_FAST,
                    Verdict::Degraded => SCORE_VERDICT_DEGRADED,
                    Verdict::Vetoed => SCORE_VERDICT_VETOED,
                    Verdict::Blocked => SCORE_VERDICT_BLOCKED,
                };

                if verdict_score > best_fitness_score
                    || (verdict_score == best_fitness_score && e.median_tps > best_fitness_tps)
                {
                    best_fitness_score = verdict_score;
                    best_fitness_tps = e.median_tps;
                    best_fitness_cold = e.cold_start_ms;
                }

                match best_verdict {
                    None => best_verdict = Some(e.verdict),
                    Some(current) if e.verdict.score() > current.score() => {
                        best_verdict = Some(e.verdict);
                    }
                    _ => {}
                }
            }
        }

        if has_fitness {
            score += best_fitness_score;
            let tps_cap = tps_bonus_cap(capability);
            score += (best_fitness_tps as i64).min(tps_cap);
            let cold_penalty = ((best_fitness_cold as i64) / 1000).min(COLD_PENALTY_CAP);
            score -= cold_penalty;
        }
    }

    if !has_fitness {
        reasoning.push("no benchmark data".to_string());
    }

    // Layer 2: Context window bonus
    let ctx_cap = context_bonus_cap(capability);
    if ctx_cap > 0 {
        if let Some(ctx) = model.context_length {
            let bonus = ((ctx as i64) / 1000).min(ctx_cap);
            score += bonus;
            if ctx >= 32_000 {
                reasoning.push(format!("{}K context", ctx / 1000));
            }
        }
    }

    // Layer 3: Model quality bonus
    let q_cap = quality_bonus_cap(capability);
    let q_mul = quality_multiplier(capability);
    if q_cap > 0 {
        let params_b = parameter_billions(model);
        if params_b > 0.0 {
            let bonus = ((params_b * q_mul as f64) as i64).min(q_cap);
            score += bonus;
            if params_b >= 3.0 {
                reasoning.push(format!("{:.0}B params", params_b));
            }
        }
    }

    // Layer 4: Name affinity
    let affinity = name_affinity_bonus(capability);
    if affinity > 0 {
        let keyword = capability.to_lowercase();
        if model.name.to_lowercase().contains(&keyword) {
            score += affinity;
            reasoning.push(format!("purpose-built {keyword} model"));
        }
    }

    Recommendation {
        model: model.name.clone(),
        rank: 0,
        score,
        pinned: false,
        verdict: best_verdict.map(|v| format!("{v:?}").to_lowercase()),
        parameter_size: model.parameter_size.clone(),
        quantization_level: model.quantization_level.clone(),
        context_length: model.context_length,
        reasoning,
    }
}

fn parameter_billions(model: &ModelInfo) -> f64 {
    if let Some(count) = model.parameter_count {
        return count as f64 / 1_000_000_000.0;
    }
    if let Some(ref size) = model.parameter_size {
        let s = size.trim().to_uppercase();
        if let Some(num) = s.strip_suffix('B') {
            if let Ok(v) = num.trim().parse::<f64>() {
                return v;
            }
        }
    }
    0.0
}

// ── Recommendation refresh helper ───────────────────────────────

/// All user-facing capability strings for recommendation computation.
pub const ALL_CAPABILITIES: &[&str] = &[
    "quick",
    "chat",
    "synthesis",
    "vision",
    "ocr",
    "tools",
    "thinking",
    "embedding",
    "imagine",
    "transcribe",
    "speak",
    "rerank",
    "translate",
];

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::*;
    use std::time::Instant;

    fn model(name: &str, caps: &[&str]) -> ModelInfo {
        ModelInfo {
            name: name.to_string(),
            parameter_count: None,
            parameter_size: Some("7B".to_string()),
            quantization_level: Some("Q4_K_M".to_string()),
            family: None,
            families: vec![],
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            format: None,
            size_disk: 4_000_000_000,
            vram_bytes: None,
            context_length: Some(8192),
        }
    }

    fn inst(name: &str, ep: &str, models: &[&str]) -> ServiceInstance {
        ServiceInstance {
            stone: Stone { id: String::new(), name: name.to_string() },
            endpoint: ep.to_string(),
            kind: OfferingKind::Ollama,
            gpu: Gpu { name: None, compute: ComputeType::Gpu },
            vram: Vram { total_bytes: 8 * 1024 * 1024 * 1024, budget_bytes: 8 * 1024 * 1024 * 1024, free_bytes: None },
            health: InstanceHealth::Healthy,
            models_available: models.iter().map(|s| s.to_string()).collect(),
            models_loaded: vec![],
            capabilities: vec![Capability::Chat],
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::Value::Null,
            priority: 0,
        }
    }

    #[test]
    fn basic_recommendation() {
        let mut models = HashMap::new();
        models.insert("llama3:8b".to_string(), model("llama3:8b", &["completion"]));
        models.insert("qwen:7b".to_string(), model("qwen:7b", &["completion"]));

        let mut instances = HashMap::new();
        instances.insert("a".to_string(), inst("s1", "a", &["llama3:8b", "qwen:7b"]));

        let matrix = GpuMatrix::default();

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 2);
        assert!(resp.selected.is_some());
    }

    #[test]
    fn pin_overrides_ranking() {
        let mut models = HashMap::new();
        models.insert("llama3:8b".to_string(), model("llama3:8b", &["completion"]));
        models.insert("qwen:7b".to_string(), model("qwen:7b", &["completion"]));

        let mut instances = HashMap::new();
        instances.insert("a".to_string(), inst("s1", "a", &["llama3:8b", "qwen:7b"]));

        let matrix = GpuMatrix::default();

        let resp = recommend("chat", &models, &instances, &matrix, Some("qwen:7b"));
        assert_eq!(resp.recommendations[0].model, "qwen:7b");
        assert!(resp.recommendations[0].pinned);
    }

    #[test]
    fn embedding_filters_correctly() {
        let mut models = HashMap::new();
        models.insert("llama3:8b".to_string(), model("llama3:8b", &["completion"]));
        models.insert("nomic".to_string(), model("nomic", &["embedding"]));

        let mut instances = HashMap::new();
        instances.insert("a".to_string(), inst("s1", "a", &["llama3:8b", "nomic"]));

        let matrix = GpuMatrix::default();

        let resp = recommend("embedding", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        assert_eq!(resp.recommendations[0].model, "nomic");
    }
}
