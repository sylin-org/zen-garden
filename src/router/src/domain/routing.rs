//! Routing algorithm: select the optimal Ollama instance for a request.
//!
//! Core principle: route to the **lowest VRAM tier** that can serve the
//! model. Overflow goes up, never down. Within a tier, pick the instance
//! with the lowest queue depth.

use super::types::{
    ModelInfo, OllamaInstance, RoutingDecision, RoutingError, Tier,
};
use std::collections::HashMap;

/// Select the best instance for a model request.
///
/// The algorithm:
/// 1. Look up the model's VRAM requirement.
/// 2. Find all tiers with enough VRAM (sorted ascending).
/// 3. In the lowest viable tier, pick the instance with the model available
///    and the lowest queue depth.
/// 4. If all instances in that tier are saturated, escalate to the next tier.
/// 5. Escalation = overflow; never route DOWN to a smaller tier.
pub fn select_instance(
    model: &str,
    instances: &HashMap<String, OllamaInstance>,
    models: &HashMap<String, ModelInfo>,
    tiers: &[Tier],
    max_queue: u32,
) -> Result<RoutingDecision, RoutingError> {
    // Do we have any healthy instances at all?
    let any_healthy = instances.values().any(|i| i.health.is_routable());
    if !any_healthy {
        return Err(RoutingError::NoHealthyInstances);
    }

    // Look up model VRAM requirement
    let vram_needed = models
        .get(model)
        .map(|m| m.vram_estimate_bytes)
        .unwrap_or(0);

    // If model is completely unknown, try to find it on any instance
    let model_exists = instances
        .values()
        .any(|i| i.models_available.iter().any(|m| m == model));

    if !model_exists && !models.contains_key(model) {
        return Err(RoutingError::ModelNotFound(model.to_string()));
    }

    // Find viable tiers (VRAM >= needed), already sorted ascending
    let viable_tiers: Vec<&Tier> = tiers.iter().filter(|t| t.vram_bytes >= vram_needed).collect();

    if viable_tiers.is_empty() {
        return Err(RoutingError::NoViableTier {
            model: model.to_string(),
            vram_needed,
        });
    }

    let lowest_tier_vram = tiers.first().map(|t| t.vram_bytes).unwrap_or(0);

    // Try each tier from lowest to highest
    for (tier_idx, tier) in viable_tiers.iter().enumerate() {
        // Find healthy instances in this tier that have the model
        let mut candidates: Vec<&OllamaInstance> = tier
            .instance_endpoints
            .iter()
            .filter_map(|ep| instances.get(ep.as_str()))
            .filter(|i| {
                i.health.is_routable() && i.models_available.iter().any(|m| m == model)
            })
            .collect();

        if candidates.is_empty() {
            continue;
        }

        // Sort by queue depth (ascending)
        candidates.sort_by_key(|i| i.queue_depth);

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
            return Ok(RoutingDecision {
                target_endpoint: inst.endpoint.clone(),
                stone_name: inst.stone_name.clone(),
                model_name: model.to_string(),
                tier_label: tier.label.clone(),
                was_overflow: tier_idx > 0,
                lease_acquired: tier.vram_bytes > lowest_tier_vram && vram_needed > lowest_tier_vram,
            });
        }
    }

    // All instances are busy
    Err(RoutingError::AllInstancesBusy {
        model: model.to_string(),
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
            ollama_version: None,
            gpu_name: None,
            vram_total_bytes: vram_gb * GIB,
            vram_budget_bytes: vram_gb * GIB,
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
            size_disk: 0,
            vram_estimate_bytes: vram_gb * GIB,
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
        let decision = select_instance("m7b", &instances, &models, &tiers, 0).unwrap();
        assert_eq!(decision.target_endpoint, "a");
        assert!(!decision.was_overflow);
    }

    #[test]
    fn overflow_to_higher_tier() {
        let mut instances = HashMap::new();
        // 8G instance doesn't have m70b
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 0));
        instances.insert("b".into(), inst("s2", "b", 24, &["m7b", "m70b"], 0));

        let models: HashMap<String, ModelInfo> =
            [("m70b", model("m70b", 20))]
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
        let decision = select_instance("m70b", &instances, &models, &tiers, 0).unwrap();
        assert_eq!(decision.target_endpoint, "b");
    }

    #[test]
    fn picks_least_loaded() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8, &["m7b"], 3));
        instances.insert("b".into(), inst("s2", "b", 8, &["m7b"], 1));

        let models: HashMap<String, ModelInfo> =
            [("m7b", model("m7b", 4))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

        let tiers = vec![Tier {
            vram_bytes: 8 * GIB,
            label: "8G".into(),
            instance_endpoints: vec!["a".into(), "b".into()],
        }];

        let decision = select_instance("m7b", &instances, &models, &tiers, 0).unwrap();
        assert_eq!(decision.target_endpoint, "b"); // lower queue depth
    }
}
