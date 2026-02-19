//! Demand-weighted model placement.
//!
//! Pure computation — no I/O, no locks. Determines the ideal
//! model→stone assignment based on recent demand distribution
//! and VRAM constraints.

use super::types::{ModelInfo, OllamaInstance, PlacementPlan};
use std::collections::HashMap;

/// Compute the ideal model→stone placement based on demand.
///
/// Algorithm:
/// 1. Rank models by demand share (descending).
/// 2. Every model with any demand gets at least 1 stone (sticky guarantee).
/// 3. Models with `demand_share × num_stones ≥ 1.5` get extra replicas.
/// 4. Assignments respect per-stone VRAM budgets (greedy bin-pack).
pub fn compute_placement(
    demand_shares: &HashMap<String, f64>,
    instances: &HashMap<String, OllamaInstance>,
    models: &HashMap<String, ModelInfo>,
) -> PlacementPlan {
    let healthy: Vec<&OllamaInstance> = instances
        .values()
        .filter(|i| i.health.is_routable())
        .collect();

    if healthy.is_empty() || demand_shares.is_empty() {
        return PlacementPlan::default();
    }

    let num_stones = healthy.len();

    // Rank models by demand (descending)
    let mut ranked: Vec<(&String, &f64)> = demand_shares.iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Compute ideal replica count per model.
    // Models whose VRAM has never been measured are skipped — we cannot
    // safely bin-pack something whose size is unknown.
    let mut ideal_replicas: Vec<(String, usize, u64)> = Vec::new();
    for (model_name, share) in &ranked {
        let vram = match models.get(model_name.as_str()).and_then(|m| m.vram_bytes) {
            Some(v) => v,
            None => {
                tracing::debug!(
                    model = %model_name,
                    "skipping model from placement — VRAM never measured"
                );
                continue;
            }
        };
        let replicas = ((**share * num_stones as f64).round() as usize).clamp(1, num_stones);
        ideal_replicas.push((model_name.to_string(), replicas, vram));
    }

    // Greedy bin-pack: track remaining VRAM per stone
    let mut stone_remaining: Vec<(String, u64)> = healthy
        .iter()
        .map(|i| (i.endpoint.clone(), i.vram_budget_bytes))
        .collect();

    let mut assignments: HashMap<String, Vec<String>> = HashMap::new();

    // Phase 1: Guarantee at least 1 replica per model (sticky).
    // Prefer stones that already have this model loaded (avoid eviction).
    for (model_name, _, vram) in &ideal_replicas {
        let preferred_idx = stone_remaining
            .iter()
            .enumerate()
            .filter(|(_, (ep, remaining))| {
                *remaining >= *vram
                    && instances
                        .get(ep)
                        .map(|i| i.models_loaded.iter().any(|l| l.name == *model_name))
                        .unwrap_or(false)
            })
            .max_by_key(|(_, (_, remaining))| *remaining)
            .map(|(idx, _)| idx);

        let idx = preferred_idx.or_else(|| {
            stone_remaining
                .iter()
                .enumerate()
                .filter(|(_, (_, remaining))| *remaining >= *vram)
                .max_by_key(|(_, (_, remaining))| *remaining)
                .map(|(idx, _)| idx)
        });

        if let Some(idx) = idx {
            let ep = stone_remaining[idx].0.clone();
            stone_remaining[idx].1 = stone_remaining[idx].1.saturating_sub(*vram);
            assignments.entry(model_name.clone()).or_default().push(ep);
        }
    }

    // Phase 2: Add extra replicas for high-demand models.
    for (model_name, replicas, vram) in &ideal_replicas {
        let current = assignments.get(model_name).map(|v| v.len()).unwrap_or(0);
        let needed = replicas.saturating_sub(current);

        for _ in 0..needed {
            let assigned = assignments.get(model_name).cloned().unwrap_or_default();
            if let Some(idx) = stone_remaining
                .iter()
                .enumerate()
                .filter(|(_, (ep, remaining))| *remaining >= *vram && !assigned.contains(ep))
                .max_by_key(|(_, (_, remaining))| *remaining)
                .map(|(idx, _)| idx)
            {
                let ep = stone_remaining[idx].0.clone();
                stone_remaining[idx].1 = stone_remaining[idx].1.saturating_sub(*vram);
                assignments.entry(model_name.clone()).or_default().push(ep);
            }
        }
    }

    PlacementPlan {
        assignments,
        computed_at: Some(chrono::Utc::now().to_rfc3339()),
        stable: false, // Caller compares with previous to set stability
    }
}

/// Check if two placement plans have equivalent assignments.
pub fn plans_equivalent(a: &PlacementPlan, b: &PlacementPlan) -> bool {
    if a.assignments.len() != b.assignments.len() {
        return false;
    }
    for (model, a_eps) in &a.assignments {
        match b.assignments.get(model) {
            Some(b_eps) => {
                let mut a_sorted = a_eps.clone();
                let mut b_sorted = b_eps.clone();
                a_sorted.sort();
                b_sorted.sort();
                if a_sorted != b_sorted {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::InstanceHealth;
    use std::time::Instant;

    const GIB: u64 = 1_073_741_824;

    fn inst(name: &str, ep: &str, vram_gb: u64) -> OllamaInstance {
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
            models_available: vec!["model-a".into(), "model-b".into()],
            queue_depth: 0,
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
    fn equal_demand_spreads_across_stones() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8));
        instances.insert("b".into(), inst("s2", "b", 8));

        let models: HashMap<String, ModelInfo> = [
            ("model-a".to_string(), model("model-a", 4)),
            ("model-b".to_string(), model("model-b", 4)),
        ]
        .into_iter()
        .collect();

        let mut demand = HashMap::new();
        demand.insert("model-a".to_string(), 0.5);
        demand.insert("model-b".to_string(), 0.5);

        let plan = compute_placement(&demand, &instances, &models);
        assert_eq!(plan.assignments.get("model-a").map(|v| v.len()), Some(1));
        assert_eq!(plan.assignments.get("model-b").map(|v| v.len()), Some(1));
        // They should be on different stones
        let a_stone = &plan.assignments["model-a"][0];
        let b_stone = &plan.assignments["model-b"][0];
        assert_ne!(a_stone, b_stone);
    }

    #[test]
    fn high_demand_gets_both_stones() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), inst("s1", "a", 8));
        instances.insert("b".into(), inst("s2", "b", 8));

        let models: HashMap<String, ModelInfo> = [
            ("model-a".to_string(), model("model-a", 4)),
            ("model-b".to_string(), model("model-b", 4)),
        ]
        .into_iter()
        .collect();

        let mut demand = HashMap::new();
        demand.insert("model-a".to_string(), 0.9);
        demand.insert("model-b".to_string(), 0.1);

        let plan = compute_placement(&demand, &instances, &models);
        // model-a: 0.9 × 2 = 1.8 → rounds to 2
        assert_eq!(plan.assignments.get("model-a").map(|v| v.len()), Some(2));
        // model-b: 0.1 × 2 = 0.2 → clamped to 1
        assert_eq!(plan.assignments.get("model-b").map(|v| v.len()), Some(1));
    }

    #[test]
    fn plans_equivalent_works() {
        let mut a = PlacementPlan::default();
        a.assignments
            .insert("m1".into(), vec!["a".into(), "b".into()]);

        let mut b = PlacementPlan::default();
        b.assignments
            .insert("m1".into(), vec!["b".into(), "a".into()]);

        assert!(plans_equivalent(&a, &b));

        b.assignments.insert("m2".into(), vec!["a".into()]);
        assert!(!plans_equivalent(&a, &b));
    }
}
