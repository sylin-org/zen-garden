//! Topology advisor: demand-weighted GPU topology recommendations.
//!
//! Pure computation — no I/O, no async, no locks.
//!
//! Given GPU inventory, model inventory, demand signals, and fitness data,
//! computes optimal placement + parallelism recommendations (ORCH-0009).
//!
//! Algorithm phases:
//! 1. **Fitness-weighted placement** — models placed on GPUs considering
//!    VRAM capacity, GPU fitness score, and workload affinity.
//! 2. **Demand-weighted parallelism** — per-GPU parallelism from VRAM
//!    headroom, weighted by the capability profile of placed models and
//!    observed demand pressure.
//! 3. **Typed recommendations** — actionable suggestions with priority,
//!    confidence, and reasoning.

use std::collections::HashMap;

use super::types::Capability;
use super::gpu_catalog::{self, FitnessSource, ResolvedFitness};

// ── Input Types ──────────────────────────────────────────────────

/// A GPU available for model placement.
#[derive(Debug, Clone)]
pub struct GpuSlot {
    /// Unique identifier (endpoint or stone name).
    pub id: String,
    /// Display name for recommendations (e.g. "stone-alpha / RTX 3060").
    pub label: String,
    /// Usable VRAM in bytes (budget, not total).
    pub vram_bytes: u64,
    /// Current parallelism setting if known.
    pub current_parallel: Option<u32>,
    /// GPU fitness score (tok/s estimate) and source.
    pub fitness: Option<ResolvedFitness>,
    /// Stone name (for demand ledger lookups).
    pub stone_name: String,
}

/// How the VRAM figure for a model was sourced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum VramSource {
    /// Projected from `size_disk × 1.1` — always available from `/api/tags`.
    Projected,
    /// Measured live from `/api/ps` while the model was loaded in VRAM.
    Measured,
}

/// A model that needs placement.
#[derive(Debug, Clone)]
pub struct ModelSlot {
    pub name: String,
    /// VRAM requirement in bytes.
    pub vram_bytes: u64,
    /// True if the model has an "embedding" capability.
    pub is_embedding: bool,
    /// Per-slot KV cache overhead estimate (bytes).
    pub kv_cache_per_slot: u64,
    /// How `vram_bytes` was sourced.
    pub vram_source: VramSource,
}

/// Demand context for the advisor (ORCH-0009).
///
/// Optional: when `None`, the advisor uses uniform demand (T=0 behavior).
#[derive(Debug, Clone, Default)]
pub struct DemandContext {
    /// Per-capability demand weight (0.0–1.0, sums to ~1.0).
    pub capability_distribution: HashMap<Capability, f64>,
    /// Per-model demand share (0.0–1.0).
    pub model_distribution: HashMap<String, f64>,
    /// Confidence level (0.0–1.0). Below 1.0, demand is blended with uniform.
    pub confidence: f64,
    /// Per-capability request rate (requests/hour) from reactive window.
    pub capability_rates: HashMap<Capability, f64>,
}

// ── Output Types ─────────────────────────────────────────────────

/// A full topology recommendation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TopologyAdvice {
    /// Per-GPU recommendations.
    pub gpus: Vec<GpuAdvice>,
    /// Free-text explanation lines for the dashboard.
    pub reasoning: Vec<String>,
    /// Overall "could improve" flag. False = current layout is fine.
    pub has_recommendations: bool,
    /// When this advice was last computed (ISO-8601).
    pub computed_at: Option<String>,
    /// What triggered this computation.
    pub trigger: String,
    /// Typed actionable recommendations (ORCH-0009).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<Recommendation>,
    /// Demand confidence (0.0–1.0) at computation time.
    pub demand_confidence: f64,
}

impl TopologyAdvice {
    /// Empty advice before first computation.
    pub fn empty() -> Self {
        Self {
            gpus: vec![],
            reasoning: vec!["Waiting for topology data...".into()],
            has_recommendations: false,
            computed_at: None,
            trigger: "none".into(),
            recommendations: vec![],
            demand_confidence: 0.0,
        }
    }
}

/// A model placed on a GPU, with its VRAM footprint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelPlacement {
    pub name: String,
    /// VRAM consumed by this model's weights (bytes).
    pub vram_bytes: u64,
}

/// Recommendation for a single GPU.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GpuAdvice {
    pub gpu_id: String,
    pub gpu_label: String,
    pub vram_bytes: u64,
    /// Models recommended for this GPU, with per-model VRAM.
    pub models: Vec<ModelPlacement>,
    /// Recommended parallelism value.
    pub recommended_parallel: u32,
    /// VRAM consumed by placed models (bytes).
    pub vram_used: u64,
    /// VRAM reserved for KV cache at recommended parallelism (bytes).
    pub vram_kv_reserved: u64,
    /// VRAM remaining after models + KV cache (bytes).
    pub vram_headroom: u64,
    /// Short rationale for parallelism choice.
    pub parallel_reason: String,
    /// Fitness source for this GPU.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness_source: Option<FitnessSource>,
    /// Fitness score (tok/s estimate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness_tps: Option<f64>,
}

/// Typed recommendation (ORCH-0009).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Recommendation {
    pub kind: RecommendationKind,
    pub priority: RecommendationPriority,
    pub stone: String,
    pub description: String,
    pub reasoning: String,
    /// How much data backs this recommendation.
    pub confidence: f64,
    /// Safe to apply without human confirmation?
    pub auto_applicable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    Parallelism,
    MaxLoadedModels,
    PlacementSwap,
    Replication,
    Eviction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationPriority {
    Urgent,
    Suggested,
    Informational,
}

// ── Constants ────────────────────────────────────────────────────

/// Default KV cache per parallel slot for chat/generate models (~300 MB).
const DEFAULT_KV_CACHE_CHAT: u64 = 300 * 1_048_576;

/// Default KV cache per parallel slot for embedding models (~80 MB).
const DEFAULT_KV_CACHE_EMBED: u64 = 80 * 1_048_576;

/// Minimum VRAM headroom to preserve for runtime overhead.
const MIN_HEADROOM: u64 = 256 * 1_048_576;

/// Maximum parallelism we'll ever recommend.
const MAX_PARALLEL: u32 = 16;

/// Chat parallelism cap when demand is unknown or mixed.
const CHAT_PARALLEL_CAP: u32 = 4;

/// Parallelism difference threshold to trigger a recommendation.
const PARALLEL_CHANGE_THRESHOLD: u32 = 2;

// ── Core Algorithm ───────────────────────────────────────────────

/// Compute topology recommendations.
///
/// When `demand` is `None`, uses uniform demand assumption (T=0).
/// When `demand` is provided, demand-weights placement and parallelism.
pub fn advise_topology(
    gpus: &[GpuSlot],
    models: &[ModelSlot],
    demand: Option<&DemandContext>,
) -> TopologyAdvice {
    if gpus.is_empty() || models.is_empty() {
        return TopologyAdvice {
            gpus: vec![],
            reasoning: vec!["No GPUs or models to evaluate.".into()],
            has_recommendations: false,
            computed_at: None,
            trigger: "none".into(),
            recommendations: vec![],
            demand_confidence: 0.0,
        };
    }

    let confidence = demand.map(|d| d.confidence).unwrap_or(0.0);

    // ── Phase 1: Fitness-weighted placement ──────────────────────
    //
    // Sort models by demand-weighted VRAM (hot models first if we have demand).
    // For each model, score all eligible GPUs and pick the best.

    let mut sorted_models: Vec<&ModelSlot> = models.iter().collect();
    sorted_models.sort_by(|a, b| {
        let a_demand = demand
            .and_then(|d| d.model_distribution.get(&a.name))
            .copied()
            .unwrap_or(0.0);
        let b_demand = demand
            .and_then(|d| d.model_distribution.get(&b.name))
            .copied()
            .unwrap_or(0.0);

        // Primary: hot models first (higher demand share)
        // Secondary: larger models first (BFD for tie-breaking)
        b_demand
            .partial_cmp(&a_demand)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.vram_bytes.cmp(&a.vram_bytes))
    });

    let mut remaining: Vec<u64> = gpus.iter().map(|g| g.vram_bytes).collect();
    let mut placed: Vec<Vec<&ModelSlot>> = vec![Vec::new(); gpus.len()];
    let mut unplaced: Vec<&ModelSlot> = Vec::new();

    for model in &sorted_models {
        // Score each GPU: fitness × (1 - utilization) / vram_fraction, filtered by capacity.
        let best = remaining
            .iter()
            .enumerate()
            .filter(|(_, r)| **r >= model.vram_bytes + MIN_HEADROOM)
            .max_by(|(idx_a, rem_a), (idx_b, rem_b)| {
                let score_a = placement_score(&gpus[*idx_a], model, **rem_a);
                let score_b = placement_score(&gpus[*idx_b], model, **rem_b);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some((idx, _)) = best {
            remaining[idx] -= model.vram_bytes;
            placed[idx].push(model);
        } else {
            unplaced.push(model);
        }
    }

    // ── Phase 2: Demand-weighted parallelism per GPU ────────────

    let mut gpu_advice: Vec<GpuAdvice> = Vec::with_capacity(gpus.len());
    let mut reasoning: Vec<String> = Vec::new();
    let mut recommendations: Vec<Recommendation> = Vec::new();

    // Compute demand weights for parallelism tuning.
    let uniform = 1.0 / Capability::ALL.len() as f64;
    let cap_demand = |cap: Capability| -> f64 {
        demand
            .and_then(|d| d.capability_distribution.get(&cap))
            .copied()
            .unwrap_or(uniform)
    };
    let embedding_demand = cap_demand(Capability::Embed);
    let thinking_demand = cap_demand(Capability::Think);

    for (idx, gpu) in gpus.iter().enumerate() {
        let gpu_models = &placed[idx];
        let vram_used: u64 = gpu_models.iter().map(|m| m.vram_bytes).sum();
        let free = gpu.vram_bytes.saturating_sub(vram_used);

        let all_embedding = !gpu_models.is_empty()
            && gpu_models.iter().all(|m| m.is_embedding);
        let has_any_embedding = gpu_models.iter().any(|m| m.is_embedding);

        // Largest KV cache cost on this GPU.
        let max_kv = gpu_models
            .iter()
            .map(|m| m.kv_cache_per_slot)
            .max()
            .unwrap_or(DEFAULT_KV_CACHE_CHAT);

        // Water-fill: max parallel slots from VRAM headroom.
        let vram_for_kv = free.saturating_sub(MIN_HEADROOM);
        let max_slots = if max_kv > 0 {
            (vram_for_kv / max_kv).min(MAX_PARALLEL as u64) as u32
        } else {
            1
        };

        // Demand-adjusted parallelism cap:
        // - All-embedding: use full water-fill (high parallelism safe)
        // - High embedding demand (>60%): allow more parallelism for mixed GPUs
        // - Thinking-heavy (>40%): reduce parallelism — sustained long generation
        //   holds KV cache for extended periods, so fewer slots preserve throughput.
        // - Otherwise: cap chat/tools workloads conservatively
        let (recommended, parallel_reason) = if gpu_models.is_empty() {
            (1, "no models placed".into())
        } else if all_embedding {
            let r = max_slots.max(1);
            (r, format!(
                "all models are embedding — high parallelism safe ({} slots × {} MB KV each)",
                r, max_kv / 1_048_576
            ))
        } else if thinking_demand > 0.4 {
            // Thinking-heavy: each request holds KV for a long generation.
            // Fewer slots ensures each slot gets maximum throughput.
            let r = max_slots.clamp(1, 2);
            (r, format!(
                "thinking-heavy workload — {} slots preserve sustained throughput (thinking demand: {:.0}%)",
                r, thinking_demand * 100.0
            ))
        } else if embedding_demand > 0.6 && has_any_embedding {
            let r = max_slots.clamp(1, 6);
            (r, format!(
                "mixed workload — {} slots (embedding demand: {:.0}%)",
                r, embedding_demand * 100.0
            ))
        } else {
            let r = max_slots.clamp(1, CHAT_PARALLEL_CAP);
            let headroom_for_msg = free.saturating_sub(max_kv * r as u64) / 1_048_576;
            (r, format!(
                "chat/generate workload — {} slots preserve context length ({} MB KV headroom)",
                r, headroom_for_msg
            ))
        };

        let kv_reserved = max_kv * recommended as u64;
        let headroom = free.saturating_sub(kv_reserved);

        // Generate typed recommendation if parallelism change needed
        if let Some(current) = gpu.current_parallel {
            let diff = (recommended as i32 - current as i32).unsigned_abs();
            if diff >= PARALLEL_CHANGE_THRESHOLD {
                let priority = if recommended > current && max_slots > current {
                    RecommendationPriority::Suggested
                } else if current > recommended && current > max_slots {
                    RecommendationPriority::Urgent // Over-provisioned, memory pressure
                } else {
                    RecommendationPriority::Suggested
                };

                let direction = if recommended > current { "increase" } else { "decrease" };
                reasoning.push(format!(
                    "{}: parallelism {} → {} recommended ({})",
                    gpu.label, current, recommended, parallel_reason
                ));

                recommendations.push(Recommendation {
                    kind: RecommendationKind::Parallelism,
                    priority,
                    stone: gpu.stone_name.clone(),
                    description: format!(
                        "{} parallelism from {} to {}",
                        direction, current, recommended
                    ),
                    reasoning: parallel_reason.clone(),
                    confidence,
                    auto_applicable: true,
                });
            }
        }

        gpu_advice.push(GpuAdvice {
            gpu_id: gpu.id.clone(),
            gpu_label: gpu.label.clone(),
            vram_bytes: gpu.vram_bytes,
            models: gpu_models
                .iter()
                .map(|m| ModelPlacement {
                    name: m.name.clone(),
                    vram_bytes: m.vram_bytes,
                })
                .collect(),
            recommended_parallel: recommended,
            vram_used,
            vram_kv_reserved: kv_reserved,
            vram_headroom: headroom,
            parallel_reason,
            fitness_source: gpu.fitness.as_ref().map(|f| f.source),
            fitness_tps: gpu.fitness.as_ref().map(|f| (f.tokens_per_sec * 10.0).round() / 10.0),
        });
    }

    // ── Phase 3: Unplaced model reporting ────────────────────────

    let max_gpu_vram = gpus.iter().map(|g| g.vram_bytes).max().unwrap_or(0);
    let mut truly_oversized: Vec<&ModelSlot> = Vec::new();
    let mut overflow_count: usize = 0;

    for m in &unplaced {
        if m.vram_bytes + MIN_HEADROOM > max_gpu_vram {
            truly_oversized.push(m);
        } else {
            overflow_count += 1;
        }
    }

    for m in &truly_oversized {
        reasoning.push(format!(
            "⚠ {} ({} MB) exceeds largest GPU ({} MB) — needs larger VRAM",
            m.name,
            m.vram_bytes / 1_048_576,
            max_gpu_vram / 1_048_576,
        ));
    }

    if overflow_count > 0 {
        reasoning.push(format!(
            "{} model(s) not placed — more models than VRAM can hold simultaneously (service loads on demand, this is normal)",
            overflow_count,
        ));
    }

    // Note VRAM data quality
    let projected_count = models
        .iter()
        .filter(|m| m.vram_source == VramSource::Projected)
        .count();
    let measured_count = models.len() - projected_count;
    if projected_count > 0 && measured_count > 0 {
        reasoning.push(format!(
            "VRAM: {} model(s) measured, {} projected from disk size.",
            measured_count, projected_count,
        ));
    } else if projected_count > 0 {
        reasoning.push(format!(
            "VRAM: all {} model(s) use projected sizes (disk × 1.1).",
            projected_count,
        ));
    }

    // Demand confidence note
    if confidence > 0.0 && confidence < 1.0 {
        reasoning.push(format!(
            "Demand confidence: {:.0}% — blending observed with uniform weights.",
            confidence * 100.0,
        ));
    } else if confidence >= 1.0 {
        reasoning.push("Demand confidence: 100% — fully data-driven.".into());
    }

    // Summary
    if recommendations.is_empty()
        && truly_oversized.is_empty()
        && gpus.iter().all(|g| {
            let advice = gpu_advice.iter().find(|a| a.gpu_id == g.id);
            advice
                .and_then(|a| {
                    g.current_parallel
                        .map(|c| (a.recommended_parallel as i32 - c as i32).unsigned_abs() < PARALLEL_CHANGE_THRESHOLD)
                })
                .unwrap_or(true)
        })
    {
        reasoning.push("Current topology looks reasonable — no changes recommended.".into());
    }

    let has_recommendations = !recommendations.is_empty() || !truly_oversized.is_empty();

    TopologyAdvice {
        gpus: gpu_advice,
        reasoning,
        has_recommendations,
        computed_at: None,
        trigger: String::new(),
        recommendations,
        demand_confidence: confidence,
    }
}

// ── Placement Scoring ───────────────────────────────────────────

/// Score a GPU for placing a given model (higher = better).
fn placement_score(gpu: &GpuSlot, _model: &ModelSlot, remaining_vram: u64) -> f64 {
    // Factor 1: GPU fitness (tok/s). Higher = faster inference.
    let fitness = gpu
        .fitness
        .as_ref()
        .map(|f| f.tokens_per_sec)
        .unwrap_or(gpu_catalog::UNKNOWN_GPU_SCORE as f64);

    // Factor 2: Remaining VRAM fraction (more headroom = better for parallelism).
    let headroom = remaining_vram as f64 / gpu.vram_bytes.max(1) as f64;

    // Combined: fitness × headroom.
    // This prefers fast GPUs with room to spare.
    fitness * (0.5 + 0.5 * headroom)
}

// ── KV Cache Estimation ─────────────────────────────────────────

/// Estimate KV cache per parallel slot for a model.
///
/// Heuristic: embedding models have tiny KV requirements; chat/generate
/// models scale with parameter count. When parameter count is unknown,
/// we use a safe default based on VRAM footprint.
pub fn estimate_kv_cache(vram_bytes: u64, is_embedding: bool, param_count: Option<u64>) -> u64 {
    if is_embedding {
        return DEFAULT_KV_CACHE_EMBED;
    }
    match param_count {
        Some(p) if p > 30_000_000_000 => 1_600 * 1_048_576,
        Some(p) if p > 10_000_000_000 => 600 * 1_048_576,
        Some(p) if p > 3_000_000_000 => 300 * 1_048_576,
        Some(_) => 150 * 1_048_576,
        None => {
            let est = vram_bytes / 25;
            est.clamp(100 * 1_048_576, 2_000 * 1_048_576)
        }
    }
}

// ── Slot Builders ───────────────────────────────────────────────

/// VRAM overhead factor when projecting from disk size.
const DISK_TO_VRAM_FACTOR: f64 = 1.1;

/// Build `ModelSlot`s for **cold (T=0) evaluation** — projected VRAM only.
pub fn model_slots_projected(
    directory: &super::types::ModelDirectory,
) -> Vec<ModelSlot> {
    directory
        .entries()
        .values()
        .filter_map(|e| {
            if e.metadata.size_disk == 0 {
                return None;
            }
            let vram = (e.metadata.size_disk as f64 * DISK_TO_VRAM_FACTOR) as u64;
            let is_embedding = e.capabilities.contains(&super::types::Capability::Embed);
            Some(ModelSlot {
                name: e.model.clone(),
                vram_bytes: vram,
                is_embedding,
                kv_cache_per_slot: estimate_kv_cache(vram, is_embedding, e.metadata.parameter_count),
                vram_source: VramSource::Projected,
            })
        })
        .collect()
}

/// Build `ModelSlot`s for **hot evaluation** — measured VRAM only.
pub fn model_slots_measured(
    directory: &super::types::ModelDirectory,
) -> Vec<ModelSlot> {
    directory
        .entries()
        .values()
        .filter_map(|e| {
            let vram = e.metadata.vram_bytes?;
            let is_embedding = e.capabilities.contains(&super::types::Capability::Embed);
            Some(ModelSlot {
                name: e.model.clone(),
                vram_bytes: vram,
                is_embedding,
                kv_cache_per_slot: estimate_kv_cache(vram, is_embedding, e.metadata.parameter_count),
                vram_source: VramSource::Measured,
            })
        })
        .collect()
}

/// Build `GpuSlot`s from the orchestrator's instance registry.
///
/// Includes fitness data when available (ORCH-0009).
pub fn gpu_slots_from_instances(
    instances: &HashMap<String, super::types::ServiceInstance>,
    demand_ledger: Option<&super::demand::DemandLedger>,
    gpu_matrix: Option<&super::fitness::GpuMatrix>,
) -> Vec<GpuSlot> {
    instances
        .values()
        .filter(|i| i.is_routable())
        .map(|i| {
            // Resolve fitness: observed (from demand ledger) > benchmarked (from gpu_matrix) > projected (from GPU name)
            let observed_tps = demand_ledger.and_then(|dl| {
                // Average observed fitness across all models on this stone
                let tps_values: Vec<f64> = i
                    .models_available
                    .iter()
                    .filter_map(|m| dl.observed_tps(m, &i.stone.name))
                    .collect();
                if tps_values.is_empty() {
                    None
                } else {
                    Some(tps_values.iter().sum::<f64>() / tps_values.len() as f64)
                }
            });

            let benchmarked_tps = gpu_matrix.and_then(|gm| {
                // Average benchmarked tps across models on this stone
                let entries: Vec<f64> = gm
                    .entries
                    .iter()
                    .filter(|e| e.endpoint == i.endpoint)
                    .filter_map(|e| {
                        if !e.verdict.is_blocked() {
                            Some(e.median_tps)
                        } else {
                            None
                        }
                    })
                    .collect();
                if entries.is_empty() {
                    None
                } else {
                    Some(entries.iter().sum::<f64>() / entries.len() as f64)
                }
            });

            let fitness = Some(gpu_catalog::resolve_fitness(
                observed_tps,
                benchmarked_tps,
                i.gpu.name.as_deref(),
            ));

            GpuSlot {
                id: i.endpoint.clone(),
                label: format!(
                    "{} / {}",
                    i.stone.name,
                    i.gpu.name.as_deref().unwrap_or("CPU")
                ),
                vram_bytes: i.vram.budget_bytes,
                current_parallel: None,
                fitness,
                stone_name: i.stone.name.clone(),
            }
        })
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1_073_741_824;
    const MIB: u64 = 1_048_576;

    fn gpu(id: &str, label: &str, vram_gib: u64, parallel: Option<u32>) -> GpuSlot {
        GpuSlot {
            id: id.into(),
            label: label.into(),
            vram_bytes: vram_gib * GIB,
            current_parallel: parallel,
            fitness: None,
            stone_name: id.into(),
        }
    }

    fn gpu_with_fitness(id: &str, label: &str, vram_gib: u64, parallel: Option<u32>, tps: f64) -> GpuSlot {
        GpuSlot {
            id: id.into(),
            label: label.into(),
            vram_bytes: vram_gib * GIB,
            current_parallel: parallel,
            fitness: Some(ResolvedFitness {
                tokens_per_sec: tps,
                source: FitnessSource::Benchmarked,
            }),
            stone_name: id.into(),
        }
    }

    fn model(name: &str, vram_mib: u64, is_embedding: bool, kv_mib: u64) -> ModelSlot {
        ModelSlot {
            name: name.into(),
            vram_bytes: vram_mib * MIB,
            is_embedding,
            kv_cache_per_slot: kv_mib * MIB,
            vram_source: VramSource::Projected,
        }
    }

    #[test]
    fn empty_inputs() {
        let advice = advise_topology(&[], &[], None);
        assert!(!advice.has_recommendations);
        assert!(advice.gpus.is_empty());
    }

    #[test]
    fn single_gpu_single_embedding_model() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![model("nomic-embed-text", 512, true, 80)];

        let advice = advise_topology(&gpus, &models, None);
        assert_eq!(advice.gpus.len(), 1);
        let g = &advice.gpus[0];
        assert_eq!(g.models.len(), 1);
        assert_eq!(g.models[0].name, "nomic-embed-text");
        assert!(
            g.recommended_parallel >= 8,
            "expected high parallelism for embedding, got {}",
            g.recommended_parallel
        );
    }

    #[test]
    fn single_gpu_single_chat_model() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![model("llama3:8b", 5000, false, 300)];

        let advice = advise_topology(&gpus, &models, None);
        let g = &advice.gpus[0];
        assert!(
            g.recommended_parallel <= 4,
            "expected capped parallelism for chat, got {}",
            g.recommended_parallel
        );
    }

    #[test]
    fn two_gpus_separates_embed_from_chat() {
        let gpus = vec![
            gpu("a", "stone-a / RTX 3060", 12, Some(1)),
            gpu("b", "stone-b / RTX 3060", 12, Some(1)),
        ];
        let models = vec![
            model("llama3:8b", 5000, false, 300),
            model("nomic-embed-text", 512, true, 80),
        ];

        let advice = advise_topology(&gpus, &models, None);

        let embed_gpu = advice
            .gpus
            .iter()
            .find(|g| g.models.iter().any(|m| m.name == "nomic-embed-text"))
            .expect("embed model should be placed");
        let chat_gpu = advice
            .gpus
            .iter()
            .find(|g| g.models.iter().any(|m| m.name == "llama3:8b"))
            .expect("chat model should be placed");

        assert!(
            embed_gpu.recommended_parallel > chat_gpu.recommended_parallel,
            "embed GPU ({}) should have higher parallelism than chat GPU ({})",
            embed_gpu.recommended_parallel,
            chat_gpu.recommended_parallel,
        );
    }

    #[test]
    fn unplaceable_model_flagged() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 4, None)];
        let models = vec![model("llama3:70b", 5000, false, 600)];

        let advice = advise_topology(&gpus, &models, None);
        assert!(advice.has_recommendations);
        assert!(
            advice.reasoning.iter().any(|r| r.contains("exceeds largest GPU")),
            "should warn about oversized model: {:?}",
            advice.reasoning
        );
    }

    #[test]
    fn many_small_models_pack_efficiently() {
        let gpus = vec![gpu("a", "stone-a / RTX 3090", 24, Some(1))];
        let models = vec![
            model("nomic-embed-text", 512, true, 80),
            model("mxbai-embed-large", 800, true, 100),
            model("all-minilm", 256, true, 50),
        ];

        let advice = advise_topology(&gpus, &models, None);
        let g = &advice.gpus[0];
        assert_eq!(g.models.len(), 3, "all embedding models should fit on 24 GB");
        assert!(
            g.recommended_parallel >= 8,
            "expected high parallelism for all-embedding GPU, got {}",
            g.recommended_parallel
        );
    }

    #[test]
    fn kv_cache_estimation() {
        assert_eq!(estimate_kv_cache(500 * MIB, true, None), 80 * MIB);
        assert_eq!(estimate_kv_cache(4500 * MIB, false, Some(7_000_000_000)), 300 * MIB);
        assert_eq!(estimate_kv_cache(40_000 * MIB, false, Some(70_000_000_000)), 1_600 * MIB);
        let est = estimate_kv_cache(4000 * MIB, false, None);
        assert!(est >= 100 * MIB && est <= 200 * MIB, "got {} MB", est / MIB);
    }

    #[test]
    fn current_parallel_triggers_recommendation() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![model("nomic-embed-text", 512, true, 80)];

        let advice = advise_topology(&gpus, &models, None);
        assert!(
            advice.has_recommendations,
            "should recommend increasing parallelism from 1 for embedding workload"
        );
        assert!(!advice.recommendations.is_empty(), "should have typed recommendations");
        assert_eq!(advice.recommendations[0].kind, RecommendationKind::Parallelism);
    }

    #[test]
    fn fitness_weighted_placement() {
        // Two GPUs same VRAM but different fitness. Hot model should go to faster GPU.
        let gpus = vec![
            gpu_with_fitness("fast", "fast-stone / RTX 4090", 12, None, 95.0),
            gpu_with_fitness("slow", "slow-stone / RTX 3060", 12, None, 48.0),
        ];
        let models = vec![model("llama3:8b", 5000, false, 300)];

        let advice = advise_topology(&gpus, &models, None);
        let placed_gpu = advice
            .gpus
            .iter()
            .find(|g| g.models.iter().any(|m| m.name == "llama3:8b"))
            .expect("model should be placed");

        assert_eq!(
            placed_gpu.gpu_id, "fast",
            "model should land on the faster GPU"
        );
    }

    #[test]
    fn demand_context_affects_parallelism() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![
            model("nomic-embed-text", 512, true, 80),
            model("llama3:8b", 5000, false, 300),
        ];

        // High embedding demand
        let demand = DemandContext {
            capability_distribution: [(Capability::Embed, 0.8), (Capability::Chat, 0.2)]
                .into_iter()
                .collect(),
            confidence: 1.0,
            ..Default::default()
        };

        let advice = advise_topology(&gpus, &models, Some(&demand));
        let g = &advice.gpus[0];
        // With 80% embedding demand on mixed GPU, should allow higher parallelism (up to 6)
        assert!(
            g.recommended_parallel > CHAT_PARALLEL_CAP,
            "expected > {} for high embedding demand, got {}",
            CHAT_PARALLEL_CAP,
            g.recommended_parallel
        );
    }
}
