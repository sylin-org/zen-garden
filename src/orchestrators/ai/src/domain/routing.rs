//! Routing algorithm: select the optimal service instance for a request.
//!
//! ## Strategy: Performance-first with demand-based reservation
//!
//! **Default (performance mode)**: flatten all candidates across tiers,
//! sort by priority, then fitness score (fastest first), then VRAM
//! (higher = faster hardware), then queue depth.
//!
//! **Reservation mode**: activated dynamically when the orchestrator
//! observes recent requests for large models that exclusively need
//! higher-tier stones. When active, small-model requests prefer
//! lower-tier candidates first.
//!
//! **Priority gate** (ORCH-0013 RT-4): if any candidate has priority >= 0,
//! all candidates with priority < 0 are excluded. Cloud providers are
//! filtered out entirely when any local instance can serve the request.
//!
//! Generalized from ollama-orchestrator domain/routing.rs — operates on
//! `ServiceInstance` and adds capability filtering + priority gate.

use std::collections::HashMap;

use super::fitness::GpuMatrix;
use super::types::{
    Capability, ModelInfo, OfferingKind, RoutingDecision, RoutingError, ServiceInstance, Stone,
    Tier,
};

/// A routing candidate: one instance on one tier.
struct Candidate<'a> {
    instance: &'a ServiceInstance,
    tier: &'a Tier,
    is_degraded: bool,
}

/// Select the best instance for a request.
///
/// The algorithm:
/// 1. Collect healthy candidates that have the model and support the capability.
/// 2. Filter out fitness-blocked candidates.
/// 3. Priority gate: exclude cloud (priority < 0) when local exists.
/// 4. Demand check: if recent traffic includes large models that exclusively
///    need higher tiers → activate reservation.
/// 5. Sort and pick.
pub fn select_instance(
    model: &str,
    capability: Option<Capability>,
    instances: &HashMap<String, ServiceInstance>,
    models: &HashMap<String, ModelInfo>,
    tiers: &[Tier],
    max_queue: u32,
    fitness: Option<&GpuMatrix>,
    recent_demand: &HashMap<String, f64>,
) -> Result<RoutingDecision, RoutingError> {
    // ── Basics ──────────────────────────────────────────────────

    let any_healthy = instances.values().any(|i| i.health.is_healthy());
    if !any_healthy {
        return Err(RoutingError::NoHealthyInstances);
    }

    // ── Capability filter ───────────────────────────────────────

    if let Some(cap) = capability {
        let any_supports = instances
            .values()
            .any(|i| i.health.is_healthy() && i.capabilities.contains(&cap));
        if !any_supports {
            return Err(RoutingError::CapabilityNotAvailable { capability: cap });
        }
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
        for ep in &tier.endpoints {
            let Some(inst) = instances.get(ep.as_str()) else {
                continue;
            };
            if !inst.health.is_healthy() {
                continue;
            }
            if !inst.models_available.iter().any(|m| m == model) {
                continue;
            }
            if let Some(cap) = capability {
                if !inst.capabilities.contains(&cap) {
                    continue;
                }
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

    // ── Priority gate (RT-4) ────────────────────────────────────
    //
    // If any candidate has priority >= 0, exclude all with priority < 0.
    // Cloud providers are filtered out when local/garden instances exist.

    let has_local = candidates.iter().any(|c| c.instance.priority >= 0);
    if has_local {
        candidates.retain(|c| c.instance.priority >= 0);
    }

    // ── Demand-based reservation check ──────────────────────────

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
        // Reservation: non-degraded first, idle before busy, lower priority desc,
        // lower VRAM tiers first, then fitness, then queue depth.
        candidates.sort_by(|a, b| {
            a.is_degraded
                .cmp(&b.is_degraded)
                .then_with(|| {
                    (a.instance.queue_depth > 0).cmp(&(b.instance.queue_depth > 0))
                })
                .then_with(|| b.instance.priority.cmp(&a.instance.priority))
                .then_with(|| a.tier.vram_bytes.cmp(&b.tier.vram_bytes))
                .then_with(|| score(&b.instance.endpoint).cmp(&score(&a.instance.endpoint)))
                .then_with(|| a.instance.queue_depth.cmp(&b.instance.queue_depth))
        });
    } else {
        // Performance-first: non-degraded first, idle before busy, priority desc,
        // highest fitness, highest VRAM, then lowest queue depth.
        candidates.sort_by(|a, b| {
            a.is_degraded
                .cmp(&b.is_degraded)
                .then_with(|| {
                    (a.instance.queue_depth > 0).cmp(&(b.instance.queue_depth > 0))
                })
                .then_with(|| b.instance.priority.cmp(&a.instance.priority))
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
            .or_else(|| candidates.iter().min_by_key(|c| c.instance.queue_depth))
    } else {
        candidates.first()
    };

    let c = chosen.ok_or_else(|| RoutingError::AllInstancesBusy {
        model: model.to_string(),
    })?;

    let uses_high_tier = c.tier.vram_bytes > lowest_tier_vram;
    let fits_low = vram_needed.map_or(false, |v| v <= lowest_tier_vram);

    Ok(RoutingDecision {
        endpoint: c.instance.endpoint.clone(),
        stone: c.instance.stone.clone(),
        model: model.to_string(),
        kind: c.instance.kind,
        tier: if c.is_degraded {
            format!("{}(degraded)", c.tier.label)
        } else {
            c.tier.label.clone()
        },
        was_overflow: reserve && uses_high_tier && fits_low,
        lease_acquired: uses_high_tier && fits_low,
    })
}

/// Find which instances have a given model (for merged model lists).
pub fn instances_with_model<'a>(
    model: &str,
    instances: &'a HashMap<String, ServiceInstance>,
) -> Vec<&'a ServiceInstance> {
    instances
        .values()
        .filter(|i| i.health.is_healthy() && i.models_available.iter().any(|m| m == model))
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::*;
    use std::time::Instant;

    const GIB: u64 = 1_073_741_824;

    fn inst(
        name: &str,
        ep: &str,
        vram_gb: u64,
        models: &[&str],
        queue: u32,
    ) -> ServiceInstance {
        ServiceInstance {
            stone: Stone {
                id: String::new(),
                name: name.to_string(),
            },
            endpoint: ep.to_string(),
            kind: OfferingKind::Ollama,
            gpu: Gpu {
                name: None,
                compute: ComputeType::Gpu,
            },
            vram: Vram {
                total_bytes: vram_gb * GIB,
                budget_bytes: vram_gb * GIB,
                free_bytes: None,
            },
            health: InstanceHealth::Healthy,
            models_available: models.iter().map(|s| s.to_string()).collect(),
            models_loaded: vec![],
            capabilities: vec![Capability::Chat, Capability::Generate],
            queue_depth: queue,
            last_seen: Instant::now(),
            metadata: serde_json::Value::Null,
            priority: 0,
        }
    }

    fn model_info(name: &str, vram_gb: u64) -> ModelInfo {
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
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m70b"], 0));

        let models: HashMap<String, ModelInfo> = [
            ("m7b".to_string(), model_info("m7b", 4)),
            ("m70b".to_string(), model_info("m70b", 20)),
        ]
        .into_iter()
        .collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into()] },
            Tier { vram_bytes: 24 * GIB, label: "24G".into(), endpoints: vec!["b".into()] },
        ];

        let d = select_instance("m7b", None, &instances, &models, &tiers, 0, None, &no_demand())
            .unwrap();
        assert_eq!(d.endpoint, "b");
        assert!(!d.was_overflow);
    }

    #[test]
    fn reservation_activates_on_large_model_demand() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m70b"], 0));

        let models: HashMap<String, ModelInfo> = [
            ("m7b".to_string(), model_info("m7b", 4)),
            ("m70b".to_string(), model_info("m70b", 20)),
        ]
        .into_iter()
        .collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into()] },
            Tier { vram_bytes: 24 * GIB, label: "24G".into(), endpoints: vec!["b".into()] },
        ];

        let demand: HashMap<String, f64> = [("m70b".to_string(), 0.3)].into_iter().collect();

        let d = select_instance("m7b", None, &instances, &models, &tiers, 0, None, &demand)
            .unwrap();
        assert_eq!(d.endpoint, "a"); // 8G tier, reservation active
    }

    #[test]
    fn priority_gate_excludes_cloud() {
        let mut instances = HashMap::new();
        instances.insert("local".into(), inst("s1", "local", 8, &["m7b"], 0));

        let mut cloud = inst("cloud-s", "cloud", 0, &["m7b"], 0);
        cloud.priority = -10;
        cloud.kind = OfferingKind::OpenAi;
        instances.insert("cloud".into(), cloud);

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 4))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["local".into()] },
            Tier { vram_bytes: 0, label: "cloud".into(), endpoints: vec!["cloud".into()] },
        ];

        let d = select_instance("m7b", None, &instances, &models, &tiers, 0, None, &no_demand())
            .unwrap();
        assert_eq!(d.endpoint, "local"); // cloud excluded by priority gate
    }

    #[test]
    fn cloud_fallback_when_no_local() {
        let mut cloud = inst("cloud-s", "cloud", 0, &["m7b"], 0);
        cloud.priority = -10;
        cloud.kind = OfferingKind::OpenAi;

        let mut instances = HashMap::new();
        instances.insert("cloud".into(), cloud);

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 0))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 0, label: "cloud".into(), endpoints: vec!["cloud".into()] },
        ];

        let d = select_instance("m7b", None, &instances, &models, &tiers, 0, None, &no_demand())
            .unwrap();
        assert_eq!(d.endpoint, "cloud"); // no local → cloud allowed
    }

    #[test]
    fn capability_filter() {
        let mut instances = HashMap::new();
        let mut ollama = inst("s1", "ollama", 8, &["m7b"], 0);
        ollama.capabilities = vec![Capability::Chat, Capability::Generate];
        instances.insert("ollama".into(), ollama);

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 4))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["ollama".into()] },
        ];

        // Transcribe capability not available
        let result = select_instance(
            "m7b",
            Some(Capability::Transcribe),
            &instances,
            &models,
            &tiers,
            0,
            None,
            &no_demand(),
        );
        assert!(matches!(result, Err(RoutingError::CapabilityNotAvailable { .. })));
    }

    #[test]
    fn picks_least_loaded() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 3));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 1));

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 4))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into(), "b".into()] },
        ];

        let d = select_instance("m7b", None, &instances, &models, &tiers, 0, None, &no_demand())
            .unwrap();
        assert_eq!(d.endpoint, "b");
    }

    #[test]
    fn safety_net_routes_oversized_model() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m48b"], 0));

        let models: HashMap<String, ModelInfo> =
            [("m48b".to_string(), model_info("m48b", 48))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into()] },
            Tier { vram_bytes: 24 * GIB, label: "24G".into(), endpoints: vec!["b".into()] },
        ];

        let d = select_instance("m48b", None, &instances, &models, &tiers, 0, None, &no_demand())
            .unwrap();
        assert_eq!(d.endpoint, "b");
        assert!(d.tier.contains("degraded"));
    }

    #[test]
    fn fitness_prefers_faster_stone() {
        use crate::domain::fitness::{GpuMatrix, GpuMatrixEntry};

        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 4))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into(), "b".into()] },
        ];

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

        let d = select_instance(
            "m7b", None, &instances, &models, &tiers, 0, Some(&matrix), &no_demand(),
        )
        .unwrap();
        assert_eq!(d.endpoint, "b");
    }

    #[test]
    fn blocked_candidates_returns_model_blocked() {
        use crate::domain::fitness::{GpuMatrix, GpuMatrixEntry};

        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));

        let models: HashMap<String, ModelInfo> =
            [("m7b".to_string(), model_info("m7b", 4))].into_iter().collect();

        let tiers = vec![
            Tier { vram_bytes: 8 * GIB, label: "8G".into(), endpoints: vec!["a".into()] },
        ];

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

        let result = select_instance(
            "m7b", None, &instances, &models, &tiers, 0, Some(&matrix), &no_demand(),
        );
        assert!(matches!(result, Err(RoutingError::ModelBlocked(_))));
    }
}
