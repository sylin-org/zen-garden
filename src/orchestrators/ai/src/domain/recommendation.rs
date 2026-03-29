//! Capability-aware model recommendations.
//!
//! Pure scoring logic — no async, no I/O.  Given model metadata, instance
//! state, and benchmark fitness data, produces a ranked list of models for
//! a requested capability.
//!
//! ## Capabilities
//!
//! - **quick** — fastest usable response (autocomplete, extraction).
//! - **chat** — best conversational quality (the default).
//! - **completion** — backward-compatible alias for `chat`.
//! - **synthesis** — long-context distillation and extraction.
//! - **tools** — function calling / agent workflows.
//! - **thinking** — extended reasoning and analysis.
//! - **embedding** — semantic search, RAG.
//! - **vision** — image understanding.
//! - **ocr** — OCR and document reading from images.
//! - **imagine** — text → image generation.
//! - **transcribe** — audio → text.
//! - **speak** — text → audio.
//! - **rerank** — query + docs → scored docs.
//! - **translate** — text + target → text.
//!
//! ## Layered Scoring
//!
//! - **Layer 0 (Availability)**: binary presence + small redundancy bonus.
//! - **Layer 1 (Fitness)**: best-stone-only verdict, throughput, cold start.
//! - **Layer 2 (Context)**: context window bonus (cap varies by capability).
//! - **Layer 3 (Quality)**: model size bonus from parameter count.

use crate::domain::types::{Capability, ModelInfo, ServiceInstance};
use crate::domain::fitness::{GpuMatrix, Verdict};
use serde::Serialize;
use std::collections::HashMap;

// ── Public types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationResponse {
    pub capability: String,
    /// The model currently selected for this capability (pinned or highest-ranked).
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

/// Maximum recommendations returned per capability.
const MAX_RECOMMENDATIONS: usize = 5;

// ── Capability mapping ──────────────────────────────────────────

/// Map a user-facing capability string to the fitness `Capability` enum.
///
/// Tools and Thinking have dedicated fitness benchmarks (ORCH-0010).
/// Falls back to Generate when no Tools/Think entry exists in the matrix.
fn fitness_capability(cap: &str) -> Option<Capability> {
    match cap {
        "tools" => Some(Capability::Tools),
        "thinking" => Some(Capability::Think),
        "quick" | "chat" | "completion" | "synthesis" => Some(Capability::Generate),
        "embedding" => Some(Capability::Embed),
        "vision" | "ocr" => Some(Capability::Vision),
        "imagine" | "edit" | "render" => Some(Capability::Generate),
        "transcribe" => Some(Capability::Generate),
        "speak" => Some(Capability::Generate),
        "rerank" => Some(Capability::Embed),
        "translate" => Some(Capability::Generate),
        _ => Some(Capability::Generate),
    }
}

/// Map a user-facing capability to the model capability tag used for filtering.
fn model_capability_tag(cap: &str) -> &str {
    match cap {
        "quick" | "chat" | "completion" | "synthesis" | "tools" | "thinking" => "completion",
        "embedding" => "embedding",
        "vision" | "ocr" => "vision",
        "imagine" => "imagine",
        "edit" => "edit",
        "render" => "render",
        "transcribe" => "transcribe",
        "speak" => "speak",
        "rerank" => "rerank",
        "translate" => "translate",
        _ => cap,
    }
}

// ── Scoring constants ───────────────────────────────────────────

// Layer 0: availability
const SCORE_AVAILABLE: i64 = 50;
const SCORE_REDUNDANCY_PER_STONE: i64 = 10;
const SCORE_REDUNDANCY_CAP: i64 = 30;
const SCORE_LOADED: i64 = 20;

// Layer 1: fitness (best-stone-only)
const SCORE_VERDICT_FAST: i64 = 300;
const SCORE_VERDICT_DEGRADED: i64 = 150;
const SCORE_VERDICT_VETOED: i64 = 30;
const SCORE_VERDICT_BLOCKED: i64 = -500;
const COLD_PENALTY_CAP: i64 = 50;

// ── Per-capability caps ─────────────────────────────────────────

/// TPS bonus cap — speed matters most for quick, less for quality-oriented caps.
fn tps_bonus_cap(cap: &str) -> i64 {
    match cap {
        "quick" => 200,
        "chat" | "completion" => 50,
        "tools" | "thinking" => 30,
        "synthesis" | "ocr" => 0, // batch workloads — speed irrelevant
        "imagine" | "edit" | "render" => 0,
        "transcribe" | "speak" => 0,
        "rerank" | "translate" => 0,
        _ => 0,
    }
}

/// Context window bonus cap — long context matters for synthesis/reasoning, not quick.
fn context_bonus_cap(cap: &str) -> i64 {
    match cap {
        "synthesis" => 500, // primary differentiator for synthesis
        "thinking" => 300,
        "tools" => 250,
        "vision" => 200,    // vision benefits from context (multi-image, complex scenes)
        "chat" | "completion" => 150,
        "ocr" => 150,
        "translate" => 100,
        "quick" => 0,
        "imagine" | "edit" | "render" => 0,
        "transcribe" | "speak" => 0,
        "rerank" => 0,
        _ => 0,
    }
}

/// Model quality bonus cap — larger models score higher for quality-oriented caps.
fn quality_bonus_cap(cap: &str) -> i64 {
    match cap {
        "thinking" => 500,
        "tools" => 450,
        "vision" => 450,   // larger vision models understand scenes better
        "chat" | "completion" | "synthesis" => 400,
        "ocr" => 400,
        "translate" => 300,
        "imagine" | "edit" | "render" => 200,
        "speak" => 100,
        "quick" => 0,
        "transcribe" => 0,
        "rerank" => 0,
        _ => 0,
    }
}

/// Quality multiplier — points per billion parameters (before cap).
fn quality_multiplier(cap: &str) -> i64 {
    match cap {
        "thinking" => 60,
        "tools" | "vision" => 50,
        "chat" | "completion" | "synthesis" => 40,
        "ocr" => 15,  // OCR: specialization > size. A tuned 1B beats a generic 13B.
        "translate" => 30,
        "imagine" | "edit" | "render" => 20,
        "speak" => 10,
        _ => 0,
    }
}

/// Name-affinity bonus — models with the capability keyword in their name
/// are purpose-built and get a significant boost.
fn name_affinity_bonus(cap: &str) -> i64 {
    match cap {
        "ocr" => 300, // models named *ocr* are purpose-built — specialization dominates
        _ => 0,
    }
}

// ── Core recommendation function ────────────────────────────────

/// Produce ranked recommendations for the given capability.
///
/// When `pin` names a model that exists in the eligible set, it is forced to
/// rank 1 regardless of score.  If the pinned model is not eligible (removed,
/// offline, wrong capability), the pin is silently ignored.
pub fn recommend(
    capability: &str,
    models: &HashMap<String, ModelInfo>,
    instances: &HashMap<String, ServiceInstance>,
    gpu_matrix: &GpuMatrix,
    pin: Option<&str>,
) -> RecommendationResponse {
    let tag = model_capability_tag(capability);
    let fitness_cap = fitness_capability(capability);

    // Filter: only models declaring the requested capability.
    // "tools" and "thinking" require the exact tag; quick/chat/completion/synthesis
    // all filter on the "completion" tag; vision/ocr filter on "vision".
    let cap_filter: &str = match capability {
        "tools" => "tools",
        "thinking" => "thinking",
        _ => tag,
    };

    let eligible: Vec<&ModelInfo> = models
        .values()
        .filter(|m| m.capabilities.iter().any(|c| c == cap_filter))
        .collect();

    // Build instance lookup: model_name → Vec<(stone_name, endpoint, loaded)>
    let mut model_stones: HashMap<&str, Vec<(&str, &str, bool)>> = HashMap::new();
    for inst in instances.values() {
        if !inst.is_routable() {
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

    // Sort by score descending, then name ascending for stability.
    scored.sort_by(|a, b| b.score.cmp(&a.score).then(a.model.cmp(&b.model)));

    // Apply pin: if the pinned model is in the list, move it to position 0.
    let pin_applied = if let Some(pinned_name) = pin {
        if let Some(pos) = scored.iter().position(|r| r.model == pinned_name) {
            let mut pinned = scored.remove(pos);
            pinned.pinned = true;
            scored.insert(0, pinned);
            true
        } else {
            false
        }
    } else {
        false
    };

    // Cap at MAX_RECOMMENDATIONS.
    scored.truncate(MAX_RECOMMENDATIONS);

    // Assign ranks.
    for (i, rec) in scored.iter_mut().enumerate() {
        rec.rank = (i + 1) as u32;
    }

    let selected = scored.first().map(|r| r.model.clone());

    // If pin was requested but not applied (model gone), include a note.
    if let Some(pinned_name) = pin {
        if !pin_applied && !scored.is_empty() {
            tracing::debug!(
                capability,
                pinned_name,
                "pinned model not eligible — ignoring pin"
            );
        }
    }

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

    // ── Layer 0: Availability ─────────────────────────────────────

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

    if loaded_count > 0 {
        if let Some(stones) = stones {
            let loaded_names: Vec<&str> = stones
                .iter()
                .filter(|(_, _, loaded)| *loaded)
                .map(|(name, _, _)| *name)
                .collect();
            reasoning.push(format!("loaded on {}", loaded_names.join(", ")));
        }
    }

    // ── Layer 1: Fitness (best stone only) ────────────────────────

    let mut has_fitness = false;
    let mut best_fitness_score: i64 = i64::MIN;
    let mut best_fitness_tps: f64 = 0.0;
    let mut best_fitness_cold: u64 = 0;
    let mut fast_count: usize = 0;
    let mut blocked_count: usize = 0;
    let mut blocked_names: Vec<String> = Vec::new();

    if let (Some(stones), Some(cap)) = (stones, fitness_cap) {
        for &(stone_name, endpoint, _loaded) in stones {
            let entry = gpu_matrix
                .entries
                .iter()
                .find(|e| {
                    e.model == model.name && e.capability == cap && e.endpoint == endpoint
                })
                // Fallback: Tools/Think → Generate when no specific entry exists
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

                // Track best stone for scoring (highest verdict, then highest TPS)
                if verdict_score > best_fitness_score
                    || (verdict_score == best_fitness_score && e.median_tps > best_fitness_tps)
                {
                    best_fitness_score = verdict_score;
                    best_fitness_tps = e.median_tps;
                    best_fitness_cold = e.cold_start_ms;
                }

                // Track best verdict for display
                match best_verdict {
                    None => best_verdict = Some(e.verdict),
                    Some(current) if e.verdict.score() > current.score() => {
                        best_verdict = Some(e.verdict);
                    }
                    _ => {}
                }

                // Count per-verdict for reasoning
                if e.verdict == Verdict::Fast {
                    fast_count += 1;
                } else if e.verdict == Verdict::Blocked {
                    blocked_count += 1;
                    blocked_names.push(stone_name.to_string());
                }
            }
        }

        // Apply best-stone fitness to score
        if has_fitness {
            score += best_fitness_score;

            let tps_cap = tps_bonus_cap(capability);
            let tps_bonus = (best_fitness_tps as i64).min(tps_cap);
            score += tps_bonus;

            let cold_penalty = ((best_fitness_cold as i64) / 1000).min(COLD_PENALTY_CAP);
            score -= cold_penalty;

            let total = stones.len();
            if fast_count > 0 {
                reasoning.push(format!("fast on {} of {} stones", fast_count, total));
            }
            if blocked_count > 0 {
                reasoning.push(format!("blocked on {}", blocked_names.join(", ")));
            }
        }
    }

    if !has_fitness {
        reasoning.push("no benchmark data — baseline score only".to_string());
    }

    // ── Layer 2: Context window bonus ─────────────────────────────

    let ctx_cap = context_bonus_cap(capability);
    if ctx_cap > 0 {
        if let Some(ctx) = model.context_length {
            let bonus = ((ctx as i64) / 1000).min(ctx_cap);
            score += bonus;
            if ctx >= 32_000 {
                reasoning.push(format!("{}K context window", ctx / 1000));
            }
        }
    }

    // ── Layer 3: Model quality bonus ──────────────────────────────

    let q_cap = quality_bonus_cap(capability);
    let q_mul = quality_multiplier(capability);
    if q_cap > 0 {
        let params_b = parameter_billions(model);
        if params_b > 0.0 {
            let bonus = ((params_b * q_mul as f64) as i64).min(q_cap);
            score += bonus;
            if params_b >= 3.0 {
                reasoning.push(format!("{:.0}B parameters", params_b));
            }
        }
    }

    // ── Layer 4: Name affinity bonus ────────────────────────────

    let affinity = name_affinity_bonus(capability);
    if affinity > 0 {
        let keyword = capability.to_lowercase();
        if model.name.to_lowercase().contains(&keyword) {
            score += affinity;
            reasoning.push(format!("purpose-built {} model", keyword));
        }
    }

    Recommendation {
        model: model.name.clone(),
        rank: 0, // assigned after sorting
        score,
        pinned: false, // set by recommend() if pin matches
        verdict: best_verdict.map(|v| v.to_string()),
        parameter_size: model.parameter_size.clone(),
        quantization_level: model.quantization_level.clone(),
        context_length: model.context_length,
        reasoning,
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Extract parameter count in billions. Prefers `parameter_count` (u64),
/// falls back to parsing `parameter_size` string (e.g. "7B" → 7.0).
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

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fitness::{GpuMatrix, GpuMatrixEntry, Verdict};
    use crate::domain::types::{
        Capability, ComputeType, Gpu, InstanceHealth, LoadedModel, ModelInfo, OfferingKind,
        ServiceInstance, Stone, Vram,
    };
    use std::time::Instant;

    fn make_model(name: &str, caps: &[&str], ctx: Option<u64>) -> ModelInfo {
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
            context_length: ctx,
        }
    }

    fn make_model_with_params(
        name: &str,
        caps: &[&str],
        ctx: Option<u64>,
        param_count: Option<u64>,
        param_size: &str,
    ) -> ModelInfo {
        ModelInfo {
            name: name.to_string(),
            parameter_count: param_count,
            parameter_size: Some(param_size.to_string()),
            quantization_level: Some("Q4_K_M".to_string()),
            family: None,
            families: vec![],
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            format: None,
            size_disk: 4_000_000_000,
            vram_bytes: None,
            context_length: ctx,
        }
    }

    fn make_instance(
        stone: &str,
        endpoint: &str,
        available: &[&str],
        loaded: &[&str],
    ) -> ServiceInstance {
        ServiceInstance {
            stone: Stone {
                id: stone.to_string(),
                name: stone.to_string(),
            },
            endpoint: endpoint.to_string(),
            kind: OfferingKind::Ollama,
            gpu: Gpu {
                name: Some("RTX 3060".to_string()),
                compute: ComputeType::Gpu,
            },
            vram: Vram {
                total_bytes: 8 * 1024 * 1024 * 1024,
                budget_bytes: 8 * 1024 * 1024 * 1024,
                free_bytes: None,
            },
            health: InstanceHealth::Healthy,
            models_loaded: loaded
                .iter()
                .map(|n| LoadedModel {
                    name: n.to_string(),
                    size_vram: 4_000_000_000,
                    expires_at: None,
                })
                .collect(),
            models_available: available.iter().map(|s| s.to_string()).collect(),
            capabilities: vec![Capability::Generate, Capability::Chat],
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::Value::Null,
            priority: 0,
        }
    }

    fn make_entry(
        model: &str,
        stone: &str,
        endpoint: &str,
        verdict: Verdict,
        tps: f64,
        cold_ms: u64,
    ) -> GpuMatrixEntry {
        make_entry_cap(model, stone, endpoint, Capability::Generate, verdict, tps, cold_ms)
    }

    fn make_entry_cap(
        model: &str,
        stone: &str,
        endpoint: &str,
        capability: Capability,
        verdict: Verdict,
        tps: f64,
        cold_ms: u64,
    ) -> GpuMatrixEntry {
        GpuMatrixEntry {
            model: model.to_string(),
            capability,
            stone_name: stone.to_string(),
            endpoint: endpoint.to_string(),
            gpu_model: "RTX 3060".to_string(),
            verdict,
            median_tps: tps,
            cold_start_ms: cold_ms,
            valid_ratio: None,
        }
    }

    #[test]
    fn filters_by_capability() {
        let mut models = HashMap::new();
        models.insert(
            "llama3:8b".to_string(),
            make_model("llama3:8b", &["completion"], Some(8192)),
        );
        models.insert(
            "nomic:latest".to_string(),
            make_model("nomic:latest", &["embedding"], Some(2048)),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("embedding", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        assert_eq!(resp.recommendations[0].model, "nomic:latest");
    }

    #[test]
    fn quick_and_chat_filter_on_completion() {
        let mut models = HashMap::new();
        models.insert(
            "llama3:8b".to_string(),
            make_model("llama3:8b", &["completion"], Some(8192)),
        );
        models.insert(
            "nomic:latest".to_string(),
            make_model("nomic:latest", &["embedding"], Some(2048)),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        // Both quick and chat should see the completion model, not the embedding one
        let quick = recommend("quick", &models, &instances, &matrix, None);
        assert_eq!(quick.recommendations.len(), 1);
        assert_eq!(quick.recommendations[0].model, "llama3:8b");

        let chat = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(chat.recommendations.len(), 1);
        assert_eq!(chat.recommendations[0].model, "llama3:8b");
    }

    #[test]
    fn distribution_gives_redundancy_bonus() {
        let mut models = HashMap::new();
        models.insert(
            "a:latest".to_string(),
            make_model("a:latest", &["completion"], None),
        );
        models.insert(
            "b:latest".to_string(),
            make_model("b:latest", &["completion"], None),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["a:latest", "b:latest"], &[]),
        );
        instances.insert(
            "s2".to_string(),
            make_instance("s2", "http://s2:11434", &["a:latest"], &[]),
        );

        let resp = recommend("chat", &models, &instances, &GpuMatrix::default(), None);
        // "a" on 2 stones gets 50 + 10 = 60, "b" on 1 stone gets 50
        assert_eq!(resp.recommendations[0].model, "a:latest");
        assert!(resp.recommendations[0].score > resp.recommendations[1].score);
        // But the gap is small (10 pts), not the old 50 pts
        let gap = resp.recommendations[0].score - resp.recommendations[1].score;
        assert_eq!(gap, SCORE_REDUNDANCY_PER_STONE);
    }

    #[test]
    fn fitness_uses_best_stone_not_sum() {
        let mut models = HashMap::new();
        models.insert(
            "m:latest".to_string(),
            make_model("m:latest", &["completion"], None),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["m:latest"], &[]),
        );
        instances.insert(
            "s2".to_string(),
            make_instance("s2", "http://s2:11434", &["m:latest"], &[]),
        );
        instances.insert(
            "s3".to_string(),
            make_instance("s3", "http://s3:11434", &["m:latest"], &[]),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("m:latest", "s1", "http://s1:11434", Verdict::Fast, 50.0, 5000),
                make_entry("m:latest", "s2", "http://s2:11434", Verdict::Fast, 60.0, 4000),
                make_entry("m:latest", "s3", "http://s3:11434", Verdict::Degraded, 10.0, 40000),
            ],
        };

        let resp = recommend("chat", &models, &instances, &matrix, None);
        let rec = &resp.recommendations[0];
        // Should use best stone (s2: Fast, 60 tps, 4s cold) — NOT sum of all 3.
        // Layer 0: 50 + 20 (2 extra × 10) = 70
        // Layer 1: 300 (fast) + min(60, 50) (tps) - 4 (cold) = 346
        // Layer 2: 0 (no context)
        // Layer 3: 280 (7B × 40)
        // Total: 696
        assert_eq!(rec.score, 70 + 300 + 50 - 4 + 280);
    }

    #[test]
    fn fitness_boosts_score() {
        let mut models = HashMap::new();
        models.insert(
            "fast:latest".to_string(),
            make_model("fast:latest", &["completion"], None),
        );
        models.insert(
            "slow:latest".to_string(),
            make_model("slow:latest", &["completion"], None),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["fast:latest", "slow:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("fast:latest", "s1", "http://s1:11434", Verdict::Fast, 100.0, 3000),
                make_entry("slow:latest", "s1", "http://s1:11434", Verdict::Vetoed, 0.5, 95000),
            ],
        };

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations[0].model, "fast:latest");
        assert!(resp.recommendations[0].score > resp.recommendations[1].score);
    }

    #[test]
    fn tools_uses_generate_fitness() {
        let mut models = HashMap::new();
        models.insert(
            "toolmodel:latest".to_string(),
            make_model("toolmodel:latest", &["tools"], Some(128_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["toolmodel:latest"], &[]),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![make_entry(
                "toolmodel:latest",
                "s1",
                "http://s1:11434",
                Verdict::Fast,
                50.0,
                5000,
            )],
        };

        let resp = recommend("tools", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        assert!(resp.recommendations[0].score > 0);
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("fast"));
        // Context bonus: min(128000/1000, 250) = 128
        assert!(resp.recommendations[0]
            .reasoning
            .iter()
            .any(|r| r.contains("128K context")));
    }

    #[test]
    fn context_bonus_for_thinking() {
        let mut models = HashMap::new();
        models.insert(
            "big:latest".to_string(),
            make_model("big:latest", &["thinking"], Some(256_000)),
        );
        models.insert(
            "small:latest".to_string(),
            make_model("small:latest", &["thinking"], Some(4_000)),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("thinking", &models, &instances, &matrix, None);
        // big gets max 256 bonus (256K/1000 = 256, now capped at 300)
        // small gets 4 bonus (4K/1000 = 4)
        assert_eq!(resp.recommendations[0].model, "big:latest");
    }

    #[test]
    fn blocked_model_ranks_low() {
        let mut models = HashMap::new();
        models.insert(
            "good:latest".to_string(),
            make_model("good:latest", &["completion"], None),
        );
        models.insert(
            "bad:latest".to_string(),
            make_model("bad:latest", &["completion"], None),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["good:latest", "bad:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("good:latest", "s1", "http://s1:11434", Verdict::Degraded, 3.0, 45000),
                make_entry("bad:latest", "s1", "http://s1:11434", Verdict::Blocked, 0.0, 999999),
            ],
        };

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations[0].model, "good:latest");
        assert!(resp.recommendations[1].score < 0); // blocked penalty dominates
    }

    #[test]
    fn chat_prefers_quality_model() {
        // Simulates: tinyllama (1.1B, fast, 120tps) vs qwen3.5 (9.7B, fast, 25tps)
        let mut models = HashMap::new();
        models.insert(
            "tinyllama:latest".to_string(),
            make_model_with_params(
                "tinyllama:latest",
                &["completion"],
                Some(2_048),
                Some(1_100_000_000),
                "1.1B",
            ),
        );
        models.insert(
            "qwen3.5:latest".to_string(),
            make_model_with_params(
                "qwen3.5:latest",
                &["completion"],
                Some(256_000),
                Some(9_700_000_000),
                "9.7B",
            ),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["tinyllama:latest", "qwen3.5:latest"],
                &[],
            ),
        );
        instances.insert(
            "s2".to_string(),
            make_instance(
                "s2",
                "http://s2:11434",
                &["tinyllama:latest", "qwen3.5:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("tinyllama:latest", "s1", "http://s1:11434", Verdict::Fast, 120.0, 3000),
                make_entry("tinyllama:latest", "s2", "http://s2:11434", Verdict::Fast, 115.0, 3500),
                make_entry("qwen3.5:latest", "s1", "http://s1:11434", Verdict::Fast, 25.0, 8000),
                make_entry("qwen3.5:latest", "s2", "http://s2:11434", Verdict::Fast, 22.0, 9000),
            ],
        };

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(
            resp.recommendations[0].model, "qwen3.5:latest",
            "Chat should prefer the larger, more capable model"
        );
    }

    #[test]
    fn quick_prefers_fast_small_model() {
        // Same setup as chat_prefers_quality_model but with "quick" capability
        let mut models = HashMap::new();
        models.insert(
            "tinyllama:latest".to_string(),
            make_model_with_params(
                "tinyllama:latest",
                &["completion"],
                Some(2_048),
                Some(1_100_000_000),
                "1.1B",
            ),
        );
        models.insert(
            "qwen3.5:latest".to_string(),
            make_model_with_params(
                "qwen3.5:latest",
                &["completion"],
                Some(256_000),
                Some(9_700_000_000),
                "9.7B",
            ),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["tinyllama:latest", "qwen3.5:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("tinyllama:latest", "s1", "http://s1:11434", Verdict::Fast, 120.0, 3000),
                make_entry("qwen3.5:latest", "s1", "http://s1:11434", Verdict::Fast, 25.0, 8000),
            ],
        };

        let resp = recommend("quick", &models, &instances, &matrix, None);
        assert_eq!(
            resp.recommendations[0].model, "tinyllama:latest",
            "Quick should prefer the faster model regardless of size"
        );
    }

    #[test]
    fn quality_layer_uses_parameter_count() {
        let mut models = HashMap::new();
        models.insert(
            "big:latest".to_string(),
            make_model_with_params(
                "big:latest",
                &["completion"],
                None,
                Some(13_000_000_000),
                "13B",
            ),
        );
        models.insert(
            "small:latest".to_string(),
            make_model_with_params(
                "small:latest",
                &["completion"],
                None,
                Some(1_000_000_000),
                "1B",
            ),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("chat", &models, &instances, &matrix, None);
        // big: quality = min(13 × 40, 400) = 400 (capped)
        // small: quality = min(1 × 40, 400) = 40
        assert_eq!(resp.recommendations[0].model, "big:latest");
        let gap = resp.recommendations[0].score - resp.recommendations[1].score;
        assert_eq!(gap, 400 - 40); // 360
    }

    #[test]
    fn quality_falls_back_to_parameter_size_string() {
        let mut models = HashMap::new();
        models.insert(
            "m:latest".to_string(),
            make_model_with_params("m:latest", &["completion"], None, None, "9.7B"),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("chat", &models, &instances, &matrix, None);
        // No parameter_count, but parameter_size = "9.7B" → 9.7 × 40 = 388
        let expected_quality = ((9.7_f64 * 40.0) as i64).min(400);
        assert!(resp.recommendations[0].score > 0);
        // Score = quality bonus only (no stones, no fitness, no context)
        assert_eq!(resp.recommendations[0].score, expected_quality);
    }

    #[test]
    fn completion_alias_for_chat() {
        let mut models = HashMap::new();
        models.insert(
            "m:latest".to_string(),
            make_model("m:latest", &["completion"], Some(32_000)),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let chat = recommend("chat", &models, &instances, &matrix, None);
        let completion = recommend("completion", &models, &instances, &matrix, None);
        assert_eq!(chat.recommendations[0].score, completion.recommendations[0].score);
    }

    #[test]
    fn synthesis_favors_long_context_over_speed() {
        // 7B/128K/60tps vs 14B/32K/20tps — Chat may favor the fast 7B,
        // Synthesis should favor whichever has better context+quality balance.
        let mut models = HashMap::new();
        models.insert(
            "fast7b:latest".to_string(),
            make_model_with_params(
                "fast7b:latest",
                &["completion"],
                Some(128_000),
                Some(7_000_000_000),
                "7B",
            ),
        );
        models.insert(
            "big14b:latest".to_string(),
            make_model_with_params(
                "big14b:latest",
                &["completion"],
                Some(32_000),
                Some(14_000_000_000),
                "14B",
            ),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["fast7b:latest", "big14b:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("fast7b:latest", "s1", "http://s1:11434", Verdict::Fast, 60.0, 5000),
                make_entry("big14b:latest", "s1", "http://s1:11434", Verdict::Fast, 20.0, 8000),
            ],
        };

        let synth = recommend("synthesis", &models, &instances, &matrix, None);

        // Synthesis: fast7b gets ctx=128 + quality=280 + tps=0
        // Synthesis: big14b gets ctx=32 + quality=400(capped) + tps=0
        // The 7B's context advantage (128 vs 32 = +96) doesn't overcome
        // the 14B's quality advantage (400 vs 280 = +120), so 14B wins.
        assert_eq!(
            synth.recommendations[0].model, "big14b:latest",
            "Synthesis should prefer quality when context gap is moderate"
        );

        // Verify TPS has zero effect on synthesis scoring
        assert_eq!(tps_bonus_cap("synthesis"), 0);
    }

    #[test]
    fn synthesis_context_cap_is_highest() {
        // Model with 256K context should get a much bigger bonus in synthesis than chat
        let mut models = HashMap::new();
        models.insert(
            "longctx:latest".to_string(),
            make_model_with_params(
                "longctx:latest",
                &["completion"],
                Some(256_000),
                Some(7_000_000_000),
                "7B",
            ),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let chat = recommend("chat", &models, &instances, &matrix, None);
        let synth = recommend("synthesis", &models, &instances, &matrix, None);

        // Chat context capped at 150, synthesis at 500
        // Chat: min(256, 150) = 150. Synthesis: min(256, 500) = 256.
        let chat_score = chat.recommendations[0].score;
        let synth_score = synth.recommendations[0].score;
        assert!(
            synth_score > chat_score,
            "Synthesis should score higher than Chat for long-context models (synth={} vs chat={})",
            synth_score,
            chat_score
        );
        // Gap should be exactly 256 - 150 = 106 (context cap difference)
        assert_eq!(synth_score - chat_score, 106);
    }

    #[test]
    fn ocr_filters_on_vision_tag() {
        let mut models = HashMap::new();
        models.insert(
            "llava:latest".to_string(),
            make_model("llava:latest", &["completion", "vision"], Some(8192)),
        );
        models.insert(
            "nomic:latest".to_string(),
            make_model("nomic:latest", &["embedding"], Some(2048)),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("ocr", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        assert_eq!(resp.recommendations[0].model, "llava:latest");
    }

    #[test]
    fn ocr_name_affinity_boosts_purpose_built_model() {
        let mut models = HashMap::new();
        models.insert(
            "llava:latest".to_string(),
            make_model_with_params(
                "llava:latest",
                &["completion", "vision"],
                Some(8192),
                Some(13_000_000_000),
                "13B",
            ),
        );
        models.insert(
            "minicpm-ocr:latest".to_string(),
            make_model_with_params(
                "minicpm-ocr:latest",
                &["completion", "vision"],
                Some(8192),
                Some(7_000_000_000),
                "7B",
            ),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("ocr", &models, &instances, &matrix, None);
        // minicpm-ocr gets +300 name affinity despite being smaller (7B vs 13B)
        // llava: quality = min(13×15, 400) = 195, no affinity = 195 total layers 3+4
        // minicpm-ocr: quality = min(7×15, 400) = 105, affinity = 300 = 405 total layers 3+4
        assert_eq!(
            resp.recommendations[0].model, "minicpm-ocr:latest",
            "OCR-named model should rank above generic vision model"
        );
        assert!(resp.recommendations[0]
            .reasoning
            .iter()
            .any(|r| r.contains("purpose-built")));
    }

    #[test]
    fn ocr_has_zero_tps_bonus() {
        assert_eq!(tps_bonus_cap("ocr"), 0);
    }

    #[test]
    fn vision_and_ocr_produce_different_rankings() {
        let mut models = HashMap::new();
        models.insert(
            "llava:latest".to_string(),
            make_model_with_params(
                "llava:latest",
                &["completion", "vision"],
                Some(32_000),
                Some(13_000_000_000),
                "13B",
            ),
        );
        models.insert(
            "minicpm-ocr:latest".to_string(),
            make_model_with_params(
                "minicpm-ocr:latest",
                &["completion", "vision"],
                Some(8_000),
                Some(7_000_000_000),
                "7B",
            ),
        );

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let vision = recommend("vision", &models, &instances, &matrix, None);
        let ocr = recommend("ocr", &models, &instances, &matrix, None);

        // Vision now has quality scoring — llava (13B) beats minicpm-ocr (7B)
        // llava: quality = min(13×50, 450) = 450, context = min(32, 200) = 32 → 482
        // minicpm-ocr: quality = min(7×50, 450) = 350, context = min(8, 200) = 8 → 358
        assert_eq!(vision.recommendations[0].model, "llava:latest");

        // OCR has quality caps + name affinity so minicpm-ocr ranks #1
        assert_eq!(
            ocr.recommendations[0].model, "minicpm-ocr:latest",
            "OCR should prefer the purpose-built model"
        );
    }

    #[test]
    fn pin_overrides_ranking() {
        let mut models = HashMap::new();
        models.insert(
            "fast:latest".to_string(),
            make_model("fast:latest", &["completion"], None),
        );
        models.insert(
            "slow:latest".to_string(),
            make_model("slow:latest", &["completion"], None),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance(
                "s1",
                "http://s1:11434",
                &["fast:latest", "slow:latest"],
                &[],
            ),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("fast:latest", "s1", "http://s1:11434", Verdict::Fast, 100.0, 3000),
                make_entry("slow:latest", "s1", "http://s1:11434", Verdict::Vetoed, 0.5, 95000),
            ],
        };

        // Without pin: fast wins
        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations[0].model, "fast:latest");
        assert!(!resp.recommendations[0].pinned);

        // With pin: slow is forced to rank 1
        let resp = recommend("chat", &models, &instances, &matrix, Some("slow:latest"));
        assert_eq!(resp.recommendations[0].model, "slow:latest");
        assert!(resp.recommendations[0].pinned);
        assert_eq!(resp.recommendations[0].rank, 1);
        assert_eq!(resp.selected.as_deref(), Some("slow:latest"));

        // Pin a model that doesn't exist — ignored
        let resp = recommend("chat", &models, &instances, &matrix, Some("missing:latest"));
        assert_eq!(resp.recommendations[0].model, "fast:latest");
        assert!(!resp.recommendations[0].pinned);
    }

    #[test]
    fn recommendations_capped_at_five() {
        let mut models = HashMap::new();
        for i in 0..8 {
            let name = format!("model{}:latest", i);
            models.insert(name.clone(), make_model(&name, &["completion"], None));
        }

        let instances = HashMap::new();
        let matrix = GpuMatrix::default();

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert!(resp.recommendations.len() <= 5);
    }

    // ── Tools/Think fitness fallback tests (ORCH-0010) ──────────

    #[test]
    fn tools_uses_dedicated_tools_entry_over_generate() {
        let mut models = HashMap::new();
        models.insert(
            "toolmodel:latest".to_string(),
            make_model("toolmodel:latest", &["completion", "tools"], Some(32_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["toolmodel:latest"], &[]),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                // Generate entry: Fast
                make_entry("toolmodel:latest", "s1", "http://s1:11434", Verdict::Fast, 50.0, 5000),
                // Tools entry: Degraded (flaky)
                make_entry_cap(
                    "toolmodel:latest", "s1", "http://s1:11434",
                    Capability::Tools, Verdict::Degraded, 45.0, 6000,
                ),
            ],
        };

        let resp = recommend("tools", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // Should use the Tools entry (Degraded), NOT the Generate entry (Fast)
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("degraded"));
    }

    #[test]
    fn tools_falls_back_to_generate_when_no_tools_entry() {
        let mut models = HashMap::new();
        models.insert(
            "toolmodel:latest".to_string(),
            make_model("toolmodel:latest", &["completion", "tools"], Some(32_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["toolmodel:latest"], &[]),
        );

        // Only a Generate entry — no Tools entry
        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("toolmodel:latest", "s1", "http://s1:11434", Verdict::Fast, 50.0, 5000),
            ],
        };

        let resp = recommend("tools", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // Should fall back to Generate entry (Fast)
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("fast"));
    }

    #[test]
    fn think_uses_dedicated_think_entry_over_generate() {
        let mut models = HashMap::new();
        models.insert(
            "thinker:latest".to_string(),
            make_model("thinker:latest", &["completion", "thinking"], Some(128_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["thinker:latest"], &[]),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                // Generate: Fast (burst is fine)
                make_entry("thinker:latest", "s1", "http://s1:11434", Verdict::Fast, 35.0, 8000),
                // Think: Vetoed (sustained throughput collapses)
                make_entry_cap(
                    "thinker:latest", "s1", "http://s1:11434",
                    Capability::Think, Verdict::Vetoed, 1.2, 50000,
                ),
            ],
        };

        let resp = recommend("thinking", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // Should use the Think entry (Vetoed), NOT the Generate entry (Fast)
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("vetoed"));
    }

    #[test]
    fn think_falls_back_to_generate_when_no_think_entry() {
        let mut models = HashMap::new();
        models.insert(
            "thinker:latest".to_string(),
            make_model("thinker:latest", &["completion", "thinking"], Some(128_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["thinker:latest"], &[]),
        );

        // Only Generate entry — no Think entry
        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry("thinker:latest", "s1", "http://s1:11434", Verdict::Fast, 35.0, 8000),
            ],
        };

        let resp = recommend("thinking", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // Should fall back to Generate (Fast)
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("fast"));
    }

    #[test]
    fn generate_does_not_fall_back_to_tools_or_think() {
        // Only Tools/Think fall back to Generate, NOT the other way around
        let mut models = HashMap::new();
        models.insert(
            "m:latest".to_string(),
            make_model("m:latest", &["completion", "tools", "thinking"], Some(32_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["m:latest"], &[]),
        );

        // Only Tools and Think entries — no Generate entry
        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                make_entry_cap("m:latest", "s1", "http://s1:11434", Capability::Tools, Verdict::Fast, 40.0, 5000),
                make_entry_cap("m:latest", "s1", "http://s1:11434", Capability::Think, Verdict::Fast, 30.0, 8000),
            ],
        };

        let resp = recommend("chat", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // No Generate entry exists and chat doesn't fall back to Tools/Think
        assert_eq!(resp.recommendations[0].verdict, None);
    }

    #[test]
    fn tools_multi_stone_uses_best_dedicated_entry() {
        let mut models = HashMap::new();
        models.insert(
            "m:latest".to_string(),
            make_model("m:latest", &["completion", "tools"], Some(32_000)),
        );

        let mut instances = HashMap::new();
        instances.insert(
            "s1".to_string(),
            make_instance("s1", "http://s1:11434", &["m:latest"], &[]),
        );
        instances.insert(
            "s2".to_string(),
            make_instance("s2", "http://s2:11434", &["m:latest"], &[]),
        );

        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                // s1: Generate Fast, Tools Degraded (flaky on Q4)
                make_entry("m:latest", "s1", "http://s1:11434", Verdict::Fast, 30.0, 5000),
                make_entry_cap("m:latest", "s1", "http://s1:11434", Capability::Tools, Verdict::Degraded, 28.0, 6000),
                // s2: Generate Fast, Tools Fast (reliable on Q8)
                make_entry("m:latest", "s2", "http://s2:11434", Verdict::Fast, 45.0, 4000),
                make_entry_cap("m:latest", "s2", "http://s2:11434", Capability::Tools, Verdict::Fast, 42.0, 4500),
            ],
        };

        let resp = recommend("tools", &models, &instances, &matrix, None);
        assert_eq!(resp.recommendations.len(), 1);
        // Best stone for tools is s2 (Fast), so overall verdict should be Fast
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("fast"));
    }
}
