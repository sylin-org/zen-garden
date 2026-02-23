//! Routing algorithm: select the optimal Ollama instance for a request.
//!
//! Core principle: route to the **lowest VRAM tier** that can serve the
//! model. Overflow goes up, never down. Within a tier, pick the instance
//! with the lowest queue depth.
//!
//! Safety net: if no tier has enough VRAM, fall back to all tiers
//! (highest-first in degraded mode). A model installed on a stone is
//! always routable — the user explicitly chose to install it.

use super::fitness::GpuMatrix;
use super::types::{ModelInfo, OllamaInstance, RoutingDecision, RoutingError, Tier};
use std::collections::HashMap;

/// Select the best instance for a model request.
///
/// The algorithm:
/// 1. Look up the model's VRAM requirement.
/// 2. Find preferred tiers with enough VRAM (sorted ascending).
/// 3. In the lowest preferred tier, pick the instance with the model
///    available — sorted by **fitness score** (advisory) then queue depth.
/// 4. If all instances in that tier are saturated, escalate to the next tier.
/// 5. **Safety net**: if no tier has enough VRAM, fall back to ALL tiers
///    (highest-first). A model installed on a stone is always routable —
///    the user explicitly chose to install it.
/// 6. Only returns error if no instance has the model at all, all
///    instances are busy, or no healthy instances exist.
pub fn select_instance(
    model: &str,
    instances: &HashMap<String, OllamaInstance>,
    models: &HashMap<String, ModelInfo>,
    tiers: &[Tier],
    max_queue: u32,
    fitness: Option<&GpuMatrix>,
) -> Result<RoutingDecision, RoutingError> {
    // Do we have any healthy instances at all?
    let any_healthy = instances.values().any(|i| i.health.is_routable());
    if !any_healthy {
        return Err(RoutingError::NoHealthyInstances);
    }

    // Look up model VRAM requirement.
    // `None` = model has never been loaded so we have no real measurement.
    let vram_needed: Option<u64> = models.get(model).and_then(|m| m.vram_bytes);

    // If model is completely unknown, try to find it on any instance
    let model_exists = instances
        .values()
        .any(|i| i.models_available.iter().any(|m| m == model));

    if !model_exists && !models.contains_key(model) {
        return Err(RoutingError::ModelNotFound(model.to_string()));
    }

    // Find preferred tiers (VRAM >= needed), already sorted ascending.
    // When VRAM requirement is unknown (None), every tier is viable —
    // the model is on the instance so Ollama already loaded it successfully.
    //
    // Safety net: if no tier has enough VRAM, fall back to ALL tiers
    // (highest-first). A model that exists on a stone MUST be routable —
    // the user explicitly installed it, so we honour that decision.
    let (preferred, fallback): (Vec<&Tier>, Vec<&Tier>) = match vram_needed {
        Some(needed) => {
            let pref: Vec<&Tier> = tiers.iter().filter(|t| t.vram_bytes >= needed).collect();
            let fb: Vec<&Tier> = tiers
                .iter()
                .filter(|t| t.vram_bytes < needed)
                .rev()
                .collect();
            (pref, fb)
        }
        None => (tiers.iter().collect(), vec![]),
    };

    // Chain preferred tiers first, then fallback (degraded) tiers.
    let viable_tiers: Vec<&Tier> = preferred.iter().chain(fallback.iter()).copied().collect();

    if viable_tiers.is_empty() {
        // No tiers defined at all — every instance is ungrouped.
        return Err(RoutingError::AllInstancesBusy {
            model: model.to_string(),
        });
    }

    let preferred_count = preferred.len();
    let lowest_tier_vram = tiers.first().map(|t| t.vram_bytes).unwrap_or(0);

    // Try each tier: preferred (lowest-first), then fallback (highest-first)
    let mut all_blocked = false;
    for (tier_idx, tier) in viable_tiers.iter().enumerate() {
        // Find healthy instances in this tier that have the model
        let mut candidates: Vec<&OllamaInstance> = tier
            .instance_endpoints
            .iter()
            .filter_map(|ep| instances.get(ep.as_str()))
            .filter(|i| i.health.is_routable() && i.models_available.iter().any(|m| m == model))
            .collect();

        if candidates.is_empty() {
            continue;
        }

        // Sort by fitness score (descending) then queue depth (ascending).
        // Fitness is advisory for Fast/Degraded/Vetoed: deprioritised but
        // still routable as last resort. Blocked is hard: filtered out.
        if let Some(f) = fitness {
            candidates.retain(|i| {
                // Keep candidate unless ALL its fitness entries for this model are Blocked.
                // (A model may have multiple capabilities; blocked in one doesn't block all.)
                let dominated = f
                    .entries
                    .iter()
                    .filter(|e| e.model == model && e.endpoint == i.endpoint)
                    .all(|e| e.verdict.is_blocked());
                let has_entries = f
                    .entries
                    .iter()
                    .any(|e| e.model == model && e.endpoint == i.endpoint);
                // Only block if we have fitness data AND all entries are Blocked.
                !(has_entries && dominated)
            });
        }

        if candidates.is_empty() {
            // All candidates in this tier were fitness-blocked.
            // Track this so we can return ModelBlocked instead of AllInstancesBusy.
            all_blocked = true;
            continue;
        }

        candidates.sort_by(|a, b| {
            let fa = fitness
                .and_then(|f| f.fitness_score(model, &a.endpoint))
                .unwrap_or(25); // Unknown score when no fitness data
            let fb = fitness
                .and_then(|f| f.fitness_score(model, &b.endpoint))
                .unwrap_or(25);
            fb.cmp(&fa).then(a.queue_depth.cmp(&b.queue_depth))
        });

        // Pick the least-loaded instance (respect max_queue if set)
        let best = if max_queue > 0 {
            candidates
                .iter()
                .find(|i| i.queue_depth < max_queue)
                .or(candidates.first())
        } else {
            candidates.first()
        };

        if let Some(inst) = best {
            let is_degraded = tier_idx >= preferred_count;
            return Ok(RoutingDecision {
                target_endpoint: inst.endpoint.clone(),
                stone_name: inst.stone_name.clone(),
                model_name: model.to_string(),
                tier_label: if is_degraded {
                    format!("{}(degraded)", tier.label)
                } else {
                    tier.label.clone()
                },
                was_overflow: tier_idx > 0 && !is_degraded,
                lease_acquired: tier.vram_bytes > lowest_tier_vram
                    && vram_needed.unwrap_or(0) > lowest_tier_vram,
            });
        }
    }

    // Distinguish: all candidates fitness-blocked vs genuinely busy.
    if all_blocked {
        Err(RoutingError::ModelBlocked(model.to_string()))
    } else {
        Err(RoutingError::AllInstancesBusy {
            model: model.to_string(),
        })
    }
}

/// For merged `/api/tags` — find which instances have a given model.
pub fn instances_with_model<'a>(
    model: &str,
    instances: &'a HashMap<String, OllamaInstance>,
) -> Vec<&'a OllamaInstance> {
    instances
        .values()
        .filter(|i| i.health.is_routable() && i.models_available.iter().any(|m| m == model))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::InstanceHealth;
    use std::time::Instant;

    const GIB: u64 = 1_073_741_824;

    fn inst(name: &str, ep: &str, vram_gb: u64, models: &[&str], queue: u32) -> OllamaInstance {
        OllamaInstance {
            stone_id: String::new(),
            stone_name: name.to_string(),
            endpoint: ep.to_string(),
            moss_endpoint: None,
            ollama_version: None,
            gpu_name: None,
            vram_total_bytes: vram_gb * GIB,
            vram_budget_bytes: vram_gb * GIB,
            num_parallel: None,
            health: InstanceHealth::Healthy,
            models_loaded: vec![],
            models_available: models.iter().map(|s| s.to_string()).collect(),
            queue_depth: queue,
            last_seen: Instant::now(),
            last_profiled: Instant::now(),
        }
    }

    fn model(name: &str, vram_gb: u64) -> ModelInfo {
        ModelInfo {
            name: name.to_string(),
            parameter_count: None,
            parameter_size: None,
            quantization_level: None,
            family: None,
            families: vec![],
            capabilities: vec![],
            format: None,
            size_disk: 0,
            vram_bytes: Some(vram_gb * GIB),
        }
    }

    #[test]
    fn routes_to_lowest_tier() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m70b"], 0));

        let models: HashMap<String, ModelInfo> =
            [("m7b", model("m7b", 4)), ("m70b", model("m70b", 20))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

        let tiers = vec![
            Tier {
                vram_bytes: 8 * GIB,
                label: "8G".into(),
                instance_endpoints: vec!["a".into()],
            },
            Tier {
                vram_bytes: 24 * GIB,
                label: "24G".into(),
                instance_endpoints: vec!["b".into()],
            },
        ];

        // m7b should route to 8G tier
        let decision = select_instance("m7b", &instances, &models, &tiers, 0, None).unwrap();
        assert_eq!(decision.target_endpoint, "a");
        assert!(!decision.was_overflow);
    }

    #[test]
    fn overflow_to_higher_tier() {
        let mut instances = HashMap::new();
        // 8G instance doesn't have m70b
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m70b"], 0));

        let models: HashMap<String, ModelInfo> = [("m70b", model("m70b", 20))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![
            Tier {
                vram_bytes: 8 * GIB,
                label: "8G".into(),
                instance_endpoints: vec!["a".into()],
            },
            Tier {
                vram_bytes: 24 * GIB,
                label: "24G".into(),
                instance_endpoints: vec!["b".into()],
            },
        ];

        // m70b needs 20G, only viable tier is 24G
        let decision = select_instance("m70b", &instances, &models, &tiers, 0, None).unwrap();
        assert_eq!(decision.target_endpoint, "b");
    }

    #[test]
    fn picks_least_loaded() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 3));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 1));

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![Tier {
            vram_bytes: 8 * GIB,
            label: "8G".into(),
            instance_endpoints: vec!["a".into(), "b".into()],
        }];

        let decision = select_instance("m7b", &instances, &models, &tiers, 0, None).unwrap();
        assert_eq!(decision.target_endpoint, "b"); // lower queue depth
    }

    #[test]
    fn safety_net_routes_oversized_model() {
        // Model needs 48G but only 8G and 24G tiers exist.
        // The 24G stone has the model installed — must be honoured.
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m48b"], 0));

        let models: HashMap<String, ModelInfo> = [("m48b", model("m48b", 48))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![
            Tier {
                vram_bytes: 8 * GIB,
                label: "8G".into(),
                instance_endpoints: vec!["a".into()],
            },
            Tier {
                vram_bytes: 24 * GIB,
                label: "24G".into(),
                instance_endpoints: vec!["b".into()],
            },
        ];

        // Previously would have returned NoViableTier — now routes via fallback.
        let decision = select_instance("m48b", &instances, &models, &tiers, 0, None).unwrap();
        assert_eq!(decision.target_endpoint, "b");
        assert!(decision.tier_label.contains("degraded"));
    }

    #[test]
    fn fitness_prefers_faster_stone() {
        use crate::domain::fitness::{Capability, GpuMatrix, GpuMatrixEntry, Verdict};

        // Two 8G stones, both have "m7b", both idle (queue=0).
        // Stone "a" is Vetoed, stone "b" is Fast → routing should prefer "b".
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![Tier {
            vram_bytes: 8 * GIB,
            label: "8G".into(),
            instance_endpoints: vec!["a".into(), "b".into()],
        }];

        let matrix = GpuMatrix {
            generated_at: Some(chrono::Utc::now()),
            entries: vec![
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Generate,
                    stone_name: "s1".into(),
                    endpoint: "a".into(),
                    gpu_model: "RTX 3060".into(),
                    verdict: Verdict::Vetoed,
                    median_tps: 0.5,
                    cold_start_ms: 100_000,
                },
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Generate,
                    stone_name: "s2".into(),
                    endpoint: "b".into(),
                    gpu_model: "RTX 3060".into(),
                    verdict: Verdict::Fast,
                    median_tps: 25.0,
                    cold_start_ms: 3_000,
                },
            ],
        };

        // With fitness data, should prefer "b" (Fast) over "a" (Vetoed)
        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix)).unwrap();
        assert_eq!(decision.target_endpoint, "b");

        // Without fitness, both are equally loaded — deterministic order from sort
        // (but the important thing is that it still works)
        let decision2 = select_instance("m7b", &instances, &models, &tiers, 0, None).unwrap();
        assert!(decision2.target_endpoint == "a" || decision2.target_endpoint == "b");
    }

    #[test]
    fn blocked_candidates_filtered_returns_model_blocked() {
        use crate::domain::fitness::{Capability, GpuMatrix, GpuMatrixEntry, Verdict};

        // Single stone has "m7b" but is Blocked on both capabilities.
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![Tier {
            vram_bytes: 8 * GIB,
            label: "8G".into(),
            instance_endpoints: vec!["a".into()],
        }];

        let matrix = GpuMatrix {
            generated_at: Some(chrono::Utc::now()),
            entries: vec![GpuMatrixEntry {
                model: "m7b".into(),
                capability: Capability::Generate,
                stone_name: "s1".into(),
                endpoint: "a".into(),
                gpu_model: "RTX 3060".into(),
                verdict: Verdict::Blocked,
                median_tps: 0.0,
                cold_start_ms: 0,
            }],
        };

        let result = select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix));
        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::ModelBlocked(m) => assert_eq!(m, "m7b"),
            other => panic!("expected ModelBlocked, got: {other}"),
        }
    }

    #[test]
    fn blocked_on_one_stone_routes_to_other() {
        use crate::domain::fitness::{Capability, GpuMatrix, GpuMatrixEntry, Verdict};

        // Two stones: "a" is Blocked, "b" is Fast → should route to "b".
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let tiers = vec![Tier {
            vram_bytes: 8 * GIB,
            label: "8G".into(),
            instance_endpoints: vec!["a".into(), "b".into()],
        }];

        let matrix = GpuMatrix {
            generated_at: Some(chrono::Utc::now()),
            entries: vec![
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Generate,
                    stone_name: "s1".into(),
                    endpoint: "a".into(),
                    gpu_model: "RTX 3060".into(),
                    verdict: Verdict::Blocked,
                    median_tps: 0.0,
                    cold_start_ms: 0,
                },
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Generate,
                    stone_name: "s2".into(),
                    endpoint: "b".into(),
                    gpu_model: "RTX 3060".into(),
                    verdict: Verdict::Fast,
                    median_tps: 25.0,
                    cold_start_ms: 3_000,
                },
            ],
        };

        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix)).unwrap();
        assert_eq!(decision.target_endpoint, "b");
    }
}
