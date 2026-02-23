//! Topology advisor: T=0 and demand-weighted GPU topology recommendations.
//!
//! Pure computation — no I/O, no async, no locks.
//!
//! Given a set of GPUs (VRAM capacities) and models (VRAM requirements +
//! capabilities), computes an optimal placement + parallelism allocation.
//!
//! Algorithm:
//! 1. **Best Fit Decreasing (BFD)** — place models on GPUs, largest first.
//! 2. **Water-fill** — allocate parallelism per GPU from VRAM headroom,
//!    weighted by the workload profile of placed models.
//! 3. Produce human-readable recommendations with reasoning.

use std::collections::HashMap;

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
    /// Current `OLLAMA_NUM_PARALLEL` if known.
    pub current_parallel: Option<u32>,
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
    /// For embedding models this is small (~50-100 MB); for chat/generate
    /// models it scales with context length (~200-400 MB for 7B).
    pub kv_cache_per_slot: u64,
    /// How `vram_bytes` was sourced.
    pub vram_source: VramSource,
}

// ── Output Types ─────────────────────────────────────────────────

/// A full topology recommendation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TopologyAdvice {
    /// Per-GPU recommendations.
    pub gpus: Vec<GpuAdvice>,
    /// Free-text explanation lines for the dashboard.
    pub reasoning: Vec<String>,
    /// Overall "could improve" flag.  False = current layout is fine.
    pub has_recommendations: bool,
    /// When this advice was last computed (ISO-8601).
    pub computed_at: Option<String>,
    /// What triggered this computation.
    pub trigger: String,
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
}

// ── Constants ────────────────────────────────────────────────────

/// Default KV cache per parallel slot for chat/generate models (~300 MB).
const DEFAULT_KV_CACHE_CHAT: u64 = 300 * 1_048_576;

/// Default KV cache per parallel slot for embedding models (~80 MB).
const DEFAULT_KV_CACHE_EMBED: u64 = 80 * 1_048_576;

/// Minimum VRAM headroom to keep free (256 MB) — Ollama overhead + safety.
const MIN_HEADROOM: u64 = 256 * 1_048_576;

/// Maximum parallelism we'll ever recommend.
const MAX_PARALLEL: u32 = 16;

// ── Core Algorithm ───────────────────────────────────────────────

/// Compute a T=0 topology recommendation (no usage history).
///
/// This is the "cold start" advisor: given only GPU capacities and model
/// requirements, produce an optimal placement + parallelism allocation.
///
/// The algorithm:
/// 1. Sort models by VRAM descending (largest first).
/// 2. For each model, pick the GPU with the **most remaining VRAM that
///    still fits** (Worst-Fit Decreasing). This spreads models across
///    GPUs, maximising per-GPU headroom for parallelism slots.
/// 3. Per GPU, compute parallelism via water-filling: divide remaining
///    VRAM by the largest per-slot KV cache cost on that GPU.
/// 4. Weight the parallelism by workload type: GPUs hosting only
///    embedding models can safely use higher parallelism.
pub fn advise_topology(gpus: &[GpuSlot], models: &[ModelSlot]) -> TopologyAdvice {
    if gpus.is_empty() || models.is_empty() {
        return TopologyAdvice {
            gpus: vec![],
            reasoning: vec!["No GPUs or models to evaluate.".into()],
            has_recommendations: false,
            computed_at: None,
            trigger: "none".into(),
        };
    }

    // ── Phase 1: Worst-Fit Decreasing placement ─────────────────
    // Spread models across GPUs (prefer most remaining VRAM) to
    // maximise per-GPU headroom available for parallelism.

    // Sort models largest-first (copy so we don't mutate input)
    let mut sorted_models: Vec<&ModelSlot> = models.iter().collect();
    sorted_models.sort_by(|a, b| b.vram_bytes.cmp(&a.vram_bytes));

    // Track remaining VRAM per GPU and which models land where.
    let mut remaining: Vec<u64> = gpus.iter().map(|g| g.vram_bytes).collect();
    let mut placed: Vec<Vec<&ModelSlot>> = vec![Vec::new(); gpus.len()];
    let mut unplaced: Vec<&ModelSlot> = Vec::new();

    for model in &sorted_models {
        // Worst-fit: largest remaining VRAM that still fits this model.
        // Spreads models across GPUs, maximising headroom for KV cache.
        let best = remaining
            .iter()
            .enumerate()
            .filter(|(_, r)| **r >= model.vram_bytes + MIN_HEADROOM)
            .max_by_key(|(_, r)| **r);

        if let Some((idx, _)) = best {
            remaining[idx] -= model.vram_bytes;
            placed[idx].push(model);
        } else {
            unplaced.push(model);
        }
    }

    // ── Phase 2: Water-fill parallelism per GPU ─────────────────

    let mut gpu_advice: Vec<GpuAdvice> = Vec::with_capacity(gpus.len());
    let mut reasoning: Vec<String> = Vec::new();

    for (idx, gpu) in gpus.iter().enumerate() {
        let gpu_models = &placed[idx];
        let vram_used: u64 = gpu_models.iter().map(|m| m.vram_bytes).sum();
        let free = gpu.vram_bytes.saturating_sub(vram_used);

        let all_embedding = !gpu_models.is_empty()
            && gpu_models.iter().all(|m| m.is_embedding);
        let has_any_embedding = gpu_models.iter().any(|m| m.is_embedding);

        // Largest KV cache cost on this GPU (determines the bottleneck).
        let max_kv = gpu_models
            .iter()
            .map(|m| m.kv_cache_per_slot)
            .max()
            .unwrap_or(DEFAULT_KV_CACHE_CHAT);

        // Water-fill: how many parallel slots fit in the free VRAM?
        let vram_for_kv = free.saturating_sub(MIN_HEADROOM);
        let max_slots = if max_kv > 0 {
            (vram_for_kv / max_kv).min(MAX_PARALLEL as u64) as u32
        } else {
            1
        };

        // Apply workload weighting:
        // - All-embedding GPUs: use full water-fill (high parallelism is safe)
        // - Mixed or all-chat GPUs: cap at lower value for latency
        let recommended = if gpu_models.is_empty() {
            1 // No models placed — default
        } else if all_embedding {
            max_slots.max(1)
        } else {
            // For chat/generate, prefer lower parallelism to preserve
            // VRAM for context length. Cap at 4 unless water-fill is lower.
            max_slots.clamp(1, 4)
        };

        let kv_reserved = max_kv * recommended as u64;
        let headroom = free.saturating_sub(kv_reserved);

        let parallel_reason = if gpu_models.is_empty() {
            "no models placed".into()
        } else if all_embedding {
            format!(
                "all models are embedding — high parallelism safe ({} slots × {} MB KV each)",
                recommended,
                max_kv / 1_048_576
            )
        } else if has_any_embedding {
            format!(
                "mixed workload — capped at {} for chat model latency ({} MB KV headroom)",
                recommended,
                headroom / 1_048_576
            )
        } else {
            format!(
                "chat/generate workload — {} slots preserve context length ({} MB KV headroom)",
                recommended,
                headroom / 1_048_576
            )
        };

        // Compare with current setting
        if let Some(current) = gpu.current_parallel {
            if recommended > current && recommended >= current + 2 {
                reasoning.push(format!(
                    "{}: parallelism {} → {} recommended ({})",
                    gpu.label, current, recommended, parallel_reason
                ));
            } else if current > recommended && current >= recommended + 2 {
                reasoning.push(format!(
                    "{}: parallelism {} → {} recommended — current may cause memory pressure ({})",
                    gpu.label, current, recommended, parallel_reason
                ));
            }
        }

        gpu_advice.push(GpuAdvice {
            gpu_id: gpu.id.clone(),
            gpu_label: gpu.label.clone(),
            vram_bytes: gpu.vram_bytes,
            models: gpu_models.iter().map(|m| ModelPlacement {
                name: m.name.clone(),
                vram_bytes: m.vram_bytes,
            }).collect(),
            recommended_parallel: recommended,
            vram_used,
            vram_kv_reserved: kv_reserved,
            vram_headroom: headroom,
            parallel_reason,
        });
    }

    // Report unplaced models
    for m in &unplaced {
        reasoning.push(format!(
            "⚠ {} ({} MB) cannot fit on any GPU — needs larger VRAM or fewer models",
            m.name,
            m.vram_bytes / 1_048_576,
        ));
    }

    // Note VRAM data quality
    let projected_count = models.iter().filter(|m| m.vram_source == VramSource::Projected).count();
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

    // Summary
    if reasoning.is_empty() && gpus.iter().all(|g| {
        let advice = gpu_advice.iter().find(|a| a.gpu_id == g.id);
        advice
            .and_then(|a| g.current_parallel.map(|c| (a.recommended_parallel as i32 - c as i32).unsigned_abs() < 2))
            .unwrap_or(true)
    }) {
        reasoning.push("Current topology looks reasonable — no changes recommended.".into());
    }

    let has_recommendations = !unplaced.is_empty()
        || gpu_advice
            .iter()
            .any(|a| {
                let gpu = gpus.iter().find(|g| g.id == a.gpu_id);
                gpu.and_then(|g| g.current_parallel)
                    .map(|c| (a.recommended_parallel as i32 - c as i32).unsigned_abs() >= 2)
                    .unwrap_or(false)
            });

    TopologyAdvice {
        gpus: gpu_advice,
        reasoning,
        has_recommendations,
        computed_at: None,
        trigger: String::new(),
    }
}

/// Estimate KV cache per parallel slot for a model.
///
/// Heuristic: embedding models have tiny KV requirements; chat/generate
/// models scale with parameter count.  When parameter count is unknown,
/// we use a safe default based on VRAM footprint.
pub fn estimate_kv_cache(vram_bytes: u64, is_embedding: bool, param_count: Option<u64>) -> u64 {
    if is_embedding {
        return DEFAULT_KV_CACHE_EMBED;
    }
    // Rough heuristic: KV cache ≈ 4% of model VRAM per slot.
    // For a 7B model (~4.5 GB VRAM) this gives ~180 MB.
    // For a 70B model (~40 GB) this gives ~1.6 GB.
    // These align with Ollama's actual KV allocation observations.
    match param_count {
        Some(p) if p > 30_000_000_000 => 1_600 * 1_048_576, // 70B+: ~1.6 GB
        Some(p) if p > 10_000_000_000 => 600 * 1_048_576,   // 13-30B: ~600 MB
        Some(p) if p > 3_000_000_000 => 300 * 1_048_576,    // 3-10B: ~300 MB
        Some(_) => 150 * 1_048_576,                          // <3B: ~150 MB
        None => {
            // No param count — estimate from VRAM footprint (4%)
            let est = vram_bytes / 25;
            est.clamp(100 * 1_048_576, 2_000 * 1_048_576)
        }
    }
}

/// VRAM overhead factor when projecting from disk size.
/// GGUF models are mmap'd, so disk size ≈ VRAM.  Add 10 % for runtime
/// overhead (activations, scratch buffers, Ollama bookkeeping).
const DISK_TO_VRAM_FACTOR: f64 = 1.1;

/// Build `ModelSlot`s for **cold (T=0) evaluation** — projected VRAM only.
///
/// Every model gets its VRAM projected from `size_disk × 1.1`.  This is
/// always available from `/api/tags` and doesn't depend on the model
/// being loaded.  Models without `size_disk` (should be rare) are skipped.
pub fn model_slots_projected(
    models: &HashMap<String, super::types::ModelInfo>,
) -> Vec<ModelSlot> {
    models
        .values()
        .filter_map(|m| {
            if m.size_disk == 0 {
                return None;
            }
            let vram = (m.size_disk as f64 * DISK_TO_VRAM_FACTOR) as u64;
            let is_embedding = m.capabilities.iter().any(|c| c == "embedding");
            Some(ModelSlot {
                name: m.name.clone(),
                vram_bytes: vram,
                is_embedding,
                kv_cache_per_slot: estimate_kv_cache(vram, is_embedding, m.parameter_count),
                vram_source: VramSource::Projected,
            })
        })
        .collect()
}

/// Build `ModelSlot`s for **hot evaluation** — measured VRAM only.
///
/// Only includes models that have been observed loaded in VRAM via
/// `/api/ps`.  Used for runtime-accurate recommendations when demand
/// history is available.
pub fn model_slots_measured(
    models: &HashMap<String, super::types::ModelInfo>,
) -> Vec<ModelSlot> {
    models
        .values()
        .filter_map(|m| {
            let vram = m.vram_bytes?;
            let is_embedding = m.capabilities.iter().any(|c| c == "embedding");
            Some(ModelSlot {
                name: m.name.clone(),
                vram_bytes: vram,
                is_embedding,
                kv_cache_per_slot: estimate_kv_cache(vram, is_embedding, m.parameter_count),
                vram_source: VramSource::Measured,
            })
        })
        .collect()
}

/// Build `GpuSlot`s from the orchestrator's instance registry.
pub fn gpu_slots_from_instances(
    instances: &HashMap<String, super::types::OllamaInstance>,
) -> Vec<GpuSlot> {
    instances
        .values()
        .filter(|i| i.health.is_routable())
        .map(|i| GpuSlot {
            id: i.endpoint.clone(),
            label: format!(
                "{} / {}",
                i.stone_name,
                i.gpu_name.as_deref().unwrap_or("CPU")
            ),
            vram_bytes: i.vram_budget_bytes,
            current_parallel: i.num_parallel,
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
        let advice = advise_topology(&[], &[]);
        assert!(!advice.has_recommendations);
        assert!(advice.gpus.is_empty());
    }

    #[test]
    fn single_gpu_single_embedding_model() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![model("nomic-embed-text", 512, true, 80)];

        let advice = advise_topology(&gpus, &models);
        assert_eq!(advice.gpus.len(), 1);
        let g = &advice.gpus[0];
        assert_eq!(g.models.len(), 1);
        assert_eq!(g.models[0].name, "nomic-embed-text");
        // With ~11.5 GB free and 80 MB KV, parallelism should be high
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

        let advice = advise_topology(&gpus, &models);
        let g = &advice.gpus[0];
        assert_eq!(g.models.len(), 1);
        assert_eq!(g.models[0].name, "llama3:8b");
        // Chat model: capped at 4 max
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

        let advice = advise_topology(&gpus, &models);

        // Large model should be placed first (BFD)
        // It will land on one GPU, embed on the other
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

        // Embed GPU should have higher parallelism than chat GPU
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
        // Model needs 5 GB but GPU only has 4 GB
        let models = vec![model("llama3:70b", 5000, false, 600)];

        let advice = advise_topology(&gpus, &models);
        assert!(advice.has_recommendations);
        assert!(
            advice.reasoning.iter().any(|r| r.contains("cannot fit")),
            "should warn about unplaceable model: {:?}",
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

        let advice = advise_topology(&gpus, &models);
        let g = &advice.gpus[0];
        assert_eq!(g.models.len(), 3, "all embedding models should fit on 24 GB");
        // All-embedding GPU: high parallelism
        assert!(
            g.recommended_parallel >= 8,
            "expected high parallelism for all-embedding GPU, got {}",
            g.recommended_parallel
        );
    }

    #[test]
    fn kv_cache_estimation() {
        // Embedding → small KV
        assert_eq!(estimate_kv_cache(500 * MIB, true, None), 80 * MIB);
        // 7B chat → ~300 MB
        assert_eq!(estimate_kv_cache(4500 * MIB, false, Some(7_000_000_000)), 300 * MIB);
        // 70B chat → ~1.6 GB
        assert_eq!(estimate_kv_cache(40_000 * MIB, false, Some(70_000_000_000)), 1_600 * MIB);
        // Unknown param count, 4 GB model → ~160 MB (4% of 4 GB)
        let est = estimate_kv_cache(4000 * MIB, false, None);
        assert!(est >= 100 * MIB && est <= 200 * MIB, "got {} MB", est / MIB);
    }

    #[test]
    fn current_parallel_triggers_recommendation() {
        let gpus = vec![gpu("a", "stone-a / RTX 3060", 12, Some(1))];
        let models = vec![model("nomic-embed-text", 512, true, 80)];

        let advice = advise_topology(&gpus, &models);
        // Current=1 but recommended should be high → has_recommendations
        assert!(
            advice.has_recommendations,
            "should recommend increasing parallelism from 1 for embedding workload"
        );
        assert!(
            advice.reasoning.iter().any(|r| r.contains("→")),
            "reasoning should suggest a change: {:?}",
            advice.reasoning
        );
    }
}
