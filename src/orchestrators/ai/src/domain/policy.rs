//! Policy evaluation: auto-pull triggers and delete-on-idle logic.
//!
//! Pure decision functions — no I/O. The caller executes the decision.

use super::types::{AutoPullMode, ModelDirectory, OrchestratorConfig, ServiceInstance};
use std::collections::HashMap;

/// Determine which models need syncing across a tier.
///
/// In `Sync` or `OnDemand` mode, every model available on *any* instance
/// in a tier should be available on *all* instances in that tier.
///
/// Returns `(model_name, target_endpoints)` pairs.
pub fn models_needing_sync(
    instances: &HashMap<String, ServiceInstance>,
    config: &OrchestratorConfig,
    directory: &ModelDirectory,
) -> Vec<(String, Vec<String>)> {
    match config.features.auto_pull_mode {
        AutoPullMode::Off => return vec![],
        AutoPullMode::Sync | AutoPullMode::OnDemand => {}
    }

    // Group instances by VRAM tier (rounded to nearest GiB).
    let mut tier_groups: HashMap<u64, Vec<&ServiceInstance>> = HashMap::new();
    for inst in instances.values() {
        if !inst.is_routable() {
            continue;
        }
        let tier_gb = inst.vram.budget_bytes / 1_073_741_824;
        tier_groups.entry(tier_gb).or_default().push(inst);
    }

    let mut sync_targets = Vec::new();

    for peers in tier_groups.values() {
        if peers.len() < 2 {
            continue;
        }

        // Union of all models in this tier
        let all_models: std::collections::HashSet<&str> = peers
            .iter()
            .flat_map(|i| i.models_available.iter().map(|m| m.as_str()))
            .collect();

        for model in all_models {
            // VRAM gate: skip models too large for the target stone.
            let model_size = directory.get(model).map(|e| e.metadata.size_disk).unwrap_or(0);
            if model_size == 0 {
                continue;
            }

            let missing_on: Vec<String> = peers
                .iter()
                .filter(|i| {
                    !i.models_available.iter().any(|m| m == model)
                        && i.vram.total_bytes > 0
                        && model_size <= i.vram.total_bytes
                })
                .map(|i| i.endpoint.clone())
                .collect();

            if !missing_on.is_empty() {
                sync_targets.push((model.to_string(), missing_on));
            }
        }
    }

    sync_targets
}

/// Should the orchestrator attempt an on-demand pull for an unknown model?
///
/// Returns `true` only in `OnDemand` mode when the model doesn't exist anywhere.
pub fn should_on_demand_pull(
    model: &str,
    instances: &HashMap<String, ServiceInstance>,
    config: &OrchestratorConfig,
) -> bool {
    if config.features.auto_pull_mode != AutoPullMode::OnDemand {
        return false;
    }

    // Model must not already exist on any instance
    !instances
        .values()
        .any(|i| i.models_available.iter().any(|m| m == model))
}

/// Identify models that should be deleted due to idle (no requests in window).
///
/// Returns (instance_endpoint, model_name) pairs.
pub fn idle_models_for_deletion(
    instances: &HashMap<String, ServiceInstance>,
    model_request_counts: &HashMap<String, u64>,
    config: &OrchestratorConfig,
) -> Vec<(String, String)> {
    if !config.features.delete_on_idle {
        return vec![];
    }

    let mut candidates = vec![];
    for inst in instances.values() {
        for model in &inst.models_available {
            let count = model_request_counts.get(model).copied().unwrap_or(0);
            if count == 0 {
                candidates.push((inst.endpoint.clone(), model.clone()));
            }
        }
    }
    candidates
}
