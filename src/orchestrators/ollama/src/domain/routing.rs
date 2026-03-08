//! Routing algorithm: select the optimal Ollama instance for a request.
//!
//! ## Strategy: Performance-first with demand-based reservation
//!
//! **Default (performance mode)**: flatten all candidates across tiers,
//! sort by fitness score (fastest first), then VRAM (higher = faster
//! hardware), then queue depth.  The most capable stone always wins —
//! delivering maximum throughput.
//!
//! **Reservation mode**: activated dynamically when the orchestrator
//! observes recent requests for large models that exclusively need
//! higher-tier stones.  When active, small-model requests prefer
//! lower-tier candidates first, keeping the high-tier stone available
//! for the large models that actually need it.
//!
//! Key: reservation is based on actual request demand (recent traffic),
//! NOT on model availability in the catalog.  If nobody is requesting
//! large models, performance mode stays active and the fastest stone
//! handles everything.
//!
//! Safety net: if no tier has enough VRAM, fall back to all tiers
//! (highest-first in degraded mode).  A model installed on a stone is
//! always routable — the user explicitly chose to install it.

use super::fitness::GpuMatrix;
use super::types::{ModelInfo, OllamaInstance, RoutingDecision, RoutingError, Tier};
use std::collections::HashMap;

/// A routing candidate: one instance on one tier.
struct Candidate<'a> {
    instance: &'a OllamaInstance,
    tier: &'a Tier,
    /// True when the tier's VRAM is below the model's requirement (safety net).
    is_degraded: bool,
}

/// Select the best instance for a model request.
///
/// The algorithm:
/// 1. Collect healthy candidates (instance + tier) that have the model.
/// 2. Filter out fitness-blocked candidates.
/// 3. **Demand check**: if recent traffic includes large models that
///    exclusively need higher tiers AND the current model fits on lower
///    tiers → activate reservation (prefer lower tiers first).
/// 4. Otherwise → performance-first (sort by fitness, then VRAM desc).
/// 5. Pick the first candidate under `max_queue`, or the least-loaded
///    as a safety fallback.
/// 6. **Safety net**: if no tier has enough VRAM, candidates from
///    smaller tiers are still included (marked degraded).
pub fn select_instance(
    model: &str,
    instances: &HashMap<String, OllamaInstance>,
    models: &HashMap<String, ModelInfo>,
    tiers: &[Tier],
    max_queue: u32,
    fitness: Option<&GpuMatrix>,
    recent_demand: &HashMap<String, f64>,
) -> Result<RoutingDecision, RoutingError> {
    // ── Basics ──────────────────────────────────────────────────

    let any_healthy = instances.values().any(|i| i.health.is_routable());
    if !any_healthy {
        return Err(RoutingError::NoHealthyInstances);
    }

    let vram_needed: Option<u64> = models.get(model).and_then(|m| m.vram_bytes);

    let model_exists = instances
        .values()
        .any(|i| i.models_available.iter().any(|m| m == model));
    if !model_exists && !models.contains_key(model) {
        return Err(RoutingError::ModelNotFound(model.to_string()));
    }

    // ── Collect candidates ──────────────────────────────────────

    let lowest_tier_vram = tiers.first().map(|t| t.vram_bytes).unwrap_or(0);
    let mut candidates: Vec<Candidate> = Vec::new();

    for tier in tiers {
        let is_degraded = vram_needed.map_or(false, |v| tier.vram_bytes < v);
        for ep in &tier.instance_endpoints {
            let Some(inst) = instances.get(ep.as_str()) else {
                continue;
            };
            if !inst.health.is_routable() {
                continue;
            }
            if !inst.models_available.iter().any(|m| m == model) {
                continue;
            }
            candidates.push(Candidate {
                instance: inst,
                tier,
                is_degraded,
            });
        }
    }

    if candidates.is_empty() {
        return Err(RoutingError::AllInstancesBusy {
            model: model.to_string(),
        });
    }

    // ── Fitness filter ──────────────────────────────────────────

    let mut all_blocked = false;
    if let Some(f) = fitness {
        let pre_len = candidates.len();
        candidates.retain(|c| {
            let dominated = f
                .entries
                .iter()
                .filter(|e| e.model == model && e.endpoint == c.instance.endpoint)
                .all(|e| e.verdict.is_blocked());
            let has_entries = f
                .entries
                .iter()
                .any(|e| e.model == model && e.endpoint == c.instance.endpoint);
            !(has_entries && dominated)
        });
        if candidates.is_empty() && pre_len > 0 {
            all_blocked = true;
        }
    }

    if candidates.is_empty() {
        return if all_blocked {
            Err(RoutingError::ModelBlocked(model.to_string()))
        } else {
            Err(RoutingError::AllInstancesBusy {
                model: model.to_string(),
            })
        };
    }

    // ── Demand-based reservation check ──────────────────────────
    //
    // Reservation activates when ALL of:
    //   1. The requested model fits on lower-tier stones.
    //   2. Candidates exist on lower tiers for this model.
    //   3. Recent demand includes a DIFFERENT model whose VRAM
    //      requirement exceeds the lowest tier (i.e. it exclusively
    //      needs high-tier stones).
    //
    // When active, lower-tier candidates are tried first so the
    // high-tier stone stays available for the large model traffic.

    let model_fits_low = vram_needed.map_or(true, |v| v <= lowest_tier_vram);
    let has_low_candidates = candidates
        .iter()
        .any(|c| c.tier.vram_bytes == lowest_tier_vram && !c.is_degraded);

    let reserve = model_fits_low
        && has_low_candidates
        && recent_demand.iter().any(|(dm, _)| {
            dm != model
                && models
                    .get(dm)
                    .and_then(|info| info.vram_bytes)
                    .map_or(false, |v| v > lowest_tier_vram)
        });

    // ── Sort candidates ─────────────────────────────────────────

    let score = |ep: &str| -> u32 {
        fitness
            .and_then(|f| f.fitness_score(model, ep))
            .unwrap_or(25)
    };

    if reserve {
        // Reservation: non-degraded first, idle before busy, then lower
        // VRAM tiers first, then fitness, then queue depth.
        candidates.sort_by(|a, b| {
            a.is_degraded
                .cmp(&b.is_degraded)
                .then_with(|| {
                    (a.instance.queue_depth > 0).cmp(&(b.instance.queue_depth > 0))
                })
                .then_with(|| a.tier.vram_bytes.cmp(&b.tier.vram_bytes))
                .then_with(|| score(&b.instance.endpoint).cmp(&score(&a.instance.endpoint)))
                .then_with(|| a.instance.queue_depth.cmp(&b.instance.queue_depth))
        });
    } else {
        // Performance-first: non-degraded first, idle before busy, then
        // highest fitness, highest VRAM, then lowest queue depth.
        candidates.sort_by(|a, b| {
            a.is_degraded
                .cmp(&b.is_degraded)
                .then_with(|| {
                    (a.instance.queue_depth > 0).cmp(&(b.instance.queue_depth > 0))
                })
                .then_with(|| score(&b.instance.endpoint).cmp(&score(&a.instance.endpoint)))
                .then_with(|| b.tier.vram_bytes.cmp(&a.tier.vram_bytes))
                .then_with(|| a.instance.queue_depth.cmp(&b.instance.queue_depth))
        });
    }

    // ── Pick ────────────────────────────────────────────────────

    let chosen = if max_queue > 0 {
        candidates
            .iter()
            .find(|c| c.instance.queue_depth < max_queue)
            .or_else(|| {
                // All saturated — pick globally least-loaded.
                candidates.iter().min_by_key(|c| c.instance.queue_depth)
            })
    } else {
        candidates.first()
    };

    let c = chosen.ok_or_else(|| RoutingError::AllInstancesBusy {
        model: model.to_string(),
    })?;

    let uses_high_tier = c.tier.vram_bytes > lowest_tier_vram;
    let fits_low = vram_needed.map_or(false, |v| v <= lowest_tier_vram);

    Ok(RoutingDecision {
        target_endpoint: c.instance.endpoint.clone(),
        stone_name: c.instance.stone_name.clone(),
        model_name: model.to_string(),
        tier_label: if c.is_degraded {
            format!("{}(degraded)", c.tier.label)
        } else {
            c.tier.label.clone()
        },
        was_overflow: reserve && uses_high_tier && fits_low,
        lease_acquired: uses_high_tier && fits_low,
    })
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
            context_length: None,
        }
    }

    fn no_demand() -> HashMap<String, f64> {
        HashMap::new()
    }

    #[test]
    fn performance_first_prefers_higher_tier() {
        // No demand → performance mode → higher VRAM (faster hardware) preferred.
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

        // m7b should route to 24G tier (performance-first, higher VRAM)
        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, None, &no_demand()).unwrap();
        assert_eq!(decision.target_endpoint, "b");
        assert!(!decision.was_overflow);
    }

    #[test]
    fn reservation_activates_on_large_model_demand() {
        // Recent demand for m70b (needs 20G) → reservation active →
        // m7b routes to 8G tier to keep 24G free.
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

        let demand: HashMap<String, f64> = [("m70b".to_string(), 0.3)].into_iter().collect();

        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, None, &demand).unwrap();
        assert_eq!(decision.target_endpoint, "a"); // 8G tier, reservation active
        assert!(!decision.was_overflow);
    }

    #[test]
    fn no_reservation_without_large_demand() {
        // Demand only contains small models → no reservation →
        // performance-first routes to 24G.
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
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

        // Demand only has m7b (small) — no large model in demand
        let demand: HashMap<String, f64> = [("m7b".to_string(), 1.0)].into_iter().collect();
        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, None, &demand).unwrap();
        assert_eq!(decision.target_endpoint, "b"); // 24G, performance-first
    }

    #[test]
    fn reservation_overflow_when_lower_saturated() {
        // Reservation active but 8G tier saturated → overflow to 24G.
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 64)); // saturated
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

        let demand: HashMap<String, f64> = [("m70b".to_string(), 0.3)].into_iter().collect();

        let decision =
            select_instance("m7b", &instances, &models, &tiers, 64, None, &demand).unwrap();
        assert_eq!(decision.target_endpoint, "b"); // overflow to 24G
        assert!(decision.was_overflow);
    }

    #[test]
    fn large_model_routes_to_viable_tier() {
        let mut instances = HashMap::new();
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
        let decision =
            select_instance("m70b", &instances, &models, &tiers, 0, None, &no_demand()).unwrap();
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

        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, None, &no_demand()).unwrap();
        assert_eq!(decision.target_endpoint, "b"); // lower queue depth
    }

    #[test]
    fn performance_spreads_to_idle_stones() {
        // 24G stone is busy (queue=1), 8G stones idle → pick idle 8G.
        // When 24G returns to idle, it wins again.
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b"], 1)); // busy

        let models: HashMap<String, ModelInfo> = [("m7b", model("m7b", 4))]
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

        // 24G busy, 8G idle → goes to idle 8G stone
        let d = select_instance("m7b", &instances, &models, &tiers, 64, None, &no_demand())
            .unwrap();
        assert_eq!(d.target_endpoint, "a");

        // Now 24G is idle again → performance picks it
        instances.get_mut("b").unwrap().queue_depth = 0;
        let d = select_instance("m7b", &instances, &models, &tiers, 64, None, &no_demand())
            .unwrap();
        assert_eq!(d.target_endpoint, "b");
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

        let decision =
            select_instance("m48b", &instances, &models, &tiers, 0, None, &no_demand()).unwrap();
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
                    valid_ratio: None,
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
                    valid_ratio: None,
                },
            ],
        };

        // With fitness data, should prefer "b" (Fast) over "a" (Vetoed)
        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix), &no_demand())
                .unwrap();
        assert_eq!(decision.target_endpoint, "b");

        // Without fitness, both are equally loaded — deterministic order from sort
        let decision2 =
            select_instance("m7b", &instances, &models, &tiers, 0, None, &no_demand()).unwrap();
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
                valid_ratio: None,
            }],
        };

        let result =
            select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix), &no_demand());
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
                    valid_ratio: None,
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
                    valid_ratio: None,
                },
            ],
        };

        let decision =
            select_instance("m7b", &instances, &models, &tiers, 0, Some(&matrix), &no_demand())
                .unwrap();
        assert_eq!(decision.target_endpoint, "b");
    }
}
