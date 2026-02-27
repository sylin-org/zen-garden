//! Capability-aware model recommendations.
//!
//! Pure scoring logic — no async, no I/O.  Given model metadata, instance
//! state, and benchmark fitness data, produces a ranked list of models for
//! a requested capability (completion, embedding, vision, tools, thinking).
//!
//! ## Layered Scoring
//!
//! - **Layer 0 (Baseline)**: distribution across stones, load state.
//! - **Layer 1 (Fitness)**: benchmark verdicts, throughput, cold start.
//! - **Layer 2 (Context)**: context window bonuses for capabilities that
//!   benefit from larger windows (tools, thinking, completion).

use crate::domain::fitness::{Capability, GpuMatrix, Verdict};
use crate::domain::types::{ModelInfo, OllamaInstance};
use serde::Serialize;
use std::collections::HashMap;

// ── Public types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationResponse {
    pub capability: String,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub model: String,
    pub rank: u32,
    pub score: i64,
    pub verdict: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub context_length: Option<u64>,
    pub reasoning: Vec<String>,
    pub stones: Vec<StoneScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoneScore {
    pub stone: String,
    pub endpoint: String,
    pub verdict: Option<String>,
    pub median_tps: Option<f64>,
    pub cold_start_ms: Option<u64>,
    pub loaded: bool,
}

// ── Capability mapping ──────────────────────────────────────────

/// Map a user-facing capability string to the fitness `Capability` enum.
/// Tools and thinking use the generate inference path.
fn fitness_capability(cap: &str) -> Option<Capability> {
    match cap {
        "completion" | "tools" | "thinking" => Some(Capability::Generate),
        "embedding" => Some(Capability::Embed),
        "vision" => Some(Capability::Vision),
        _ => None,
    }
}

/// Map a user-facing capability to the string Ollama uses in model capabilities.
fn ollama_capability(cap: &str) -> &str {
    match cap {
        "completion" | "tools" | "thinking" => "completion",
        "embedding" => "embedding",
        "vision" => "vision",
        _ => cap,
    }
}

// ── Scoring constants ───────────────────────────────────────────

const SCORE_PER_AVAILABLE_STONE: i64 = 50;
const SCORE_PER_LOADED_STONE: i64 = 30;

const SCORE_VERDICT_FAST: i64 = 400;
const SCORE_VERDICT_DEGRADED: i64 = 200;
const SCORE_VERDICT_VETOED: i64 = 50;
const SCORE_VERDICT_BLOCKED: i64 = -1000;

const TPS_BONUS_CAP: i64 = 200;
const COLD_PENALTY_CAP: i64 = 100;

// ── Core recommendation function ────────────────────────────────

/// Produce ranked recommendations for the given capability.
pub fn recommend(
    capability: &str,
    models: &HashMap<String, ModelInfo>,
    instances: &HashMap<String, OllamaInstance>,
    gpu_matrix: &GpuMatrix,
) -> RecommendationResponse {
    let ollama_cap = ollama_capability(capability);
    let fitness_cap = fitness_capability(capability);

    // Filter: only models declaring the requested capability.
    // Special case: "tools" and "thinking" — check the exact Ollama capability string.
    let cap_filter: &str = match capability {
        "tools" => "tools",
        "thinking" => "thinking",
        _ => ollama_cap,
    };

    let eligible: Vec<&ModelInfo> = models
        .values()
        .filter(|m| m.capabilities.iter().any(|c| c == cap_filter))
        .collect();

    // Build instance lookup: model_name → Vec<(stone_name, endpoint, loaded)>
    let mut model_stones: HashMap<&str, Vec<(&str, &str, bool)>> = HashMap::new();
    for inst in instances.values() {
        if !inst.health.is_routable() {
            continue;
        }
        for model_name in &inst.models_available {
            let loaded = inst.models_loaded.iter().any(|l| l.name == *model_name);
            model_stones
                .entry(model_name.as_str())
                .or_default()
                .push((&inst.stone_name, &inst.endpoint, loaded));
        }
    }

    let mut scored: Vec<Recommendation> = eligible
        .iter()
        .map(|m| score_model(m, capability, fitness_cap, &model_stones, gpu_matrix))
        .collect();

    // Sort by score descending, then name ascending for stability.
    scored.sort_by(|a, b| b.score.cmp(&a.score).then(a.model.cmp(&b.model)));

    // Assign ranks.
    for (i, rec) in scored.iter_mut().enumerate() {
        rec.rank = (i + 1) as u32;
    }

    RecommendationResponse {
        capability: capability.to_string(),
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
    let mut stone_scores: Vec<StoneScore> = Vec::new();
    let mut best_verdict: Option<Verdict> = None;

    // ── Layer 0: Baseline (distribution) ────────────────────────

    let stones = model_stones.get(model.name.as_str());
    let available_count = stones.map(|s| s.len()).unwrap_or(0);
    let loaded_count = stones
        .map(|s| s.iter().filter(|(_, _, loaded)| *loaded).count())
        .unwrap_or(0);

    score += available_count as i64 * SCORE_PER_AVAILABLE_STONE;
    score += loaded_count as i64 * SCORE_PER_LOADED_STONE;

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

    // ── Layer 1: Fitness (benchmark data) ───────────────────────

    let mut has_fitness = false;

    if let (Some(stones), Some(cap)) = (stones, fitness_cap) {
        for &(stone_name, endpoint, loaded) in stones {
            // Find fitness entry for this (model, capability, stone)
            let entry = gpu_matrix.entries.iter().find(|e| {
                e.model == model.name && e.capability == cap && e.endpoint == endpoint
            });

            let mut ss = StoneScore {
                stone: stone_name.to_string(),
                endpoint: endpoint.to_string(),
                verdict: None,
                median_tps: None,
                cold_start_ms: None,
                loaded,
            };

            if let Some(e) = entry {
                has_fitness = true;
                ss.verdict = Some(e.verdict.to_string());
                ss.median_tps = Some(e.median_tps);
                ss.cold_start_ms = Some(e.cold_start_ms);

                // Verdict score
                let verdict_score = match e.verdict {
                    Verdict::Fast => SCORE_VERDICT_FAST,
                    Verdict::Degraded => SCORE_VERDICT_DEGRADED,
                    Verdict::Vetoed => SCORE_VERDICT_VETOED,
                    Verdict::Blocked => SCORE_VERDICT_BLOCKED,
                };
                score += verdict_score;

                // Throughput bonus (capped)
                let tps_bonus = (e.median_tps as i64).min(TPS_BONUS_CAP);
                score += tps_bonus;

                // Cold start penalty (capped)
                let cold_penalty =
                    ((e.cold_start_ms as i64) / 1000).min(COLD_PENALTY_CAP);
                score -= cold_penalty;

                // Track best verdict
                match best_verdict {
                    None => best_verdict = Some(e.verdict),
                    Some(current) if e.verdict.score() > current.score() => {
                        best_verdict = Some(e.verdict);
                    }
                    _ => {}
                }
            }

            stone_scores.push(ss);
        }

        // Fitness reasoning
        if has_fitness {
            let fast_count = stone_scores
                .iter()
                .filter(|s| s.verdict.as_deref() == Some("fast"))
                .count();
            let blocked_count = stone_scores
                .iter()
                .filter(|s| s.verdict.as_deref() == Some("blocked"))
                .count();
            let total = stone_scores.len();

            if fast_count > 0 {
                reasoning.push(format!("fast on {} of {} stones", fast_count, total));
            }
            if blocked_count > 0 {
                let names: Vec<&str> = stone_scores
                    .iter()
                    .filter(|s| s.verdict.as_deref() == Some("blocked"))
                    .map(|s| s.stone.as_str())
                    .collect();
                reasoning.push(format!("blocked on {}", names.join(", ")));
            }
        }
    } else if let Some(stones) = stones {
        // No fitness capability mapping — just populate stone entries
        for &(stone_name, endpoint, loaded) in stones {
            stone_scores.push(StoneScore {
                stone: stone_name.to_string(),
                endpoint: endpoint.to_string(),
                verdict: None,
                median_tps: None,
                cold_start_ms: None,
                loaded,
            });
        }
    }

    if !has_fitness {
        reasoning.push("no benchmark data — baseline score only".to_string());
    }

    // ── Layer 2: Context-aware bonuses ──────────────────────────

    let context_bonus_cap: i64 = match capability {
        "thinking" => 200,
        "tools" => 150,
        "completion" => 100,
        _ => 0,
    };

    if context_bonus_cap > 0 {
        if let Some(ctx) = model.context_length {
            let bonus = ((ctx as i64) / 1000).min(context_bonus_cap);
            score += bonus;
            if ctx >= 32_000 {
                reasoning.push(format!("{}K context window", ctx / 1000));
            }
        }
    }

    // Sort stones: best verdict first, then loaded first
    stone_scores.sort_by(|a, b| {
        let va = verdict_rank(a.verdict.as_deref());
        let vb = verdict_rank(b.verdict.as_deref());
        vb.cmp(&va)
            .then(b.loaded.cmp(&a.loaded))
            .then(a.stone.cmp(&b.stone))
    });

    Recommendation {
        model: model.name.clone(),
        rank: 0, // assigned after sorting
        score,
        verdict: best_verdict.map(|v| v.to_string()),
        parameter_size: model.parameter_size.clone(),
        quantization_level: model.quantization_level.clone(),
        context_length: model.context_length,
        reasoning,
        stones: stone_scores,
    }
}

fn verdict_rank(v: Option<&str>) -> u32 {
    match v {
        Some("fast") => 4,
        Some("degraded") => 3,
        Some("vetoed") => 2,
        Some("blocked") => 1,
        _ => 0,
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fitness::{Capability, GpuMatrix, GpuMatrixEntry, Verdict};
    use crate::domain::types::{InstanceHealth, LoadedModel, ModelInfo, OllamaInstance};
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

    fn make_instance(
        stone: &str,
        endpoint: &str,
        available: &[&str],
        loaded: &[&str],
    ) -> OllamaInstance {
        OllamaInstance {
            stone_id: stone.to_string(),
            stone_name: stone.to_string(),
            endpoint: endpoint.to_string(),
            moss_endpoint: None,
            ollama_version: None,
            gpu_name: Some("RTX 3060".to_string()),
            vram_total_bytes: 8 * 1024 * 1024 * 1024,
            vram_budget_bytes: 8 * 1024 * 1024 * 1024,
            num_parallel: None,
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
            queue_depth: 0,
            last_seen: Instant::now(),
            last_profiled: Instant::now(),
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

        let resp = recommend("embedding", &models, &instances, &matrix);
        assert_eq!(resp.recommendations.len(), 1);
        assert_eq!(resp.recommendations[0].model, "nomic:latest");
    }

    #[test]
    fn distribution_scores_higher() {
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

        let resp = recommend("completion", &models, &instances, &GpuMatrix::default());
        // "a" is on 2 stones, "b" on 1 → "a" ranks higher
        assert_eq!(resp.recommendations[0].model, "a:latest");
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
                GpuMatrixEntry {
                    model: "fast:latest".to_string(),
                    capability: Capability::Generate,
                    stone_name: "s1".to_string(),
                    endpoint: "http://s1:11434".to_string(),
                    gpu_model: "RTX 3060".to_string(),
                    verdict: Verdict::Fast,
                    median_tps: 100.0,
                    cold_start_ms: 3000,
                },
                GpuMatrixEntry {
                    model: "slow:latest".to_string(),
                    capability: Capability::Generate,
                    stone_name: "s1".to_string(),
                    endpoint: "http://s1:11434".to_string(),
                    gpu_model: "RTX 3060".to_string(),
                    verdict: Verdict::Vetoed,
                    median_tps: 0.5,
                    cold_start_ms: 95000,
                },
            ],
        };

        let resp = recommend("completion", &models, &instances, &matrix);
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
            entries: vec![GpuMatrixEntry {
                model: "toolmodel:latest".to_string(),
                capability: Capability::Generate,
                stone_name: "s1".to_string(),
                endpoint: "http://s1:11434".to_string(),
                gpu_model: "RTX 3060".to_string(),
                verdict: Verdict::Fast,
                median_tps: 50.0,
                cold_start_ms: 5000,
            }],
        };

        let resp = recommend("tools", &models, &instances, &matrix);
        assert_eq!(resp.recommendations.len(), 1);
        // Should have fitness data (from generate) AND context bonus
        assert!(resp.recommendations[0].score > 0);
        assert_eq!(resp.recommendations[0].verdict.as_deref(), Some("fast"));
        // Context bonus: min(128000/1000, 150) = 128
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

        let resp = recommend("thinking", &models, &instances, &matrix);
        // big gets max 200 bonus (256K/1000 = 256, capped at 200)
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
                GpuMatrixEntry {
                    model: "good:latest".to_string(),
                    capability: Capability::Generate,
                    stone_name: "s1".to_string(),
                    endpoint: "http://s1:11434".to_string(),
                    gpu_model: "RTX 3060".to_string(),
                    verdict: Verdict::Degraded,
                    median_tps: 3.0,
                    cold_start_ms: 45000,
                },
                GpuMatrixEntry {
                    model: "bad:latest".to_string(),
                    capability: Capability::Generate,
                    stone_name: "s1".to_string(),
                    endpoint: "http://s1:11434".to_string(),
                    gpu_model: "RTX 3060".to_string(),
                    verdict: Verdict::Blocked,
                    median_tps: 0.0,
                    cold_start_ms: 999999,
                },
            ],
        };

        let resp = recommend("completion", &models, &instances, &matrix);
        assert_eq!(resp.recommendations[0].model, "good:latest");
        assert!(resp.recommendations[1].score < 0); // blocked penalty dominates
    }
}
