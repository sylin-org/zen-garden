//! Policy evaluation: auto-pull triggers and delete-on-idle logic.
//!
//! Pure decision functions — no I/O. The caller executes the decision.

use super::types::{ModelInfo, OllamaInstance, RouterConfig};
use std::collections::HashMap;

/// Should the router auto-pull a model that was requested but missing?
///
/// Returns the list of target instance endpoints if auto-pull should fire.
pub fn should_auto_pull(
    model: &str,
    instances: &HashMap<String, OllamaInstance>,
    _models: &HashMap<String, ModelInfo>,
    config: &RouterConfig,
) -> Vec<String> {
    if !config.features.auto_pull {
        return vec![];
    }

    // Model must not already exist on any instance
    let exists_anywhere = instances
        .values()
        .any(|i| i.models_available.iter().any(|m| m == model));
    if exists_anywhere {
        return vec![];
    }

    // Pick all healthy instances (we don't know the model size yet,
    // so we can't filter by VRAM — Ollama will handle that).
    // For now, pull to all healthy instances.
    instances
        .values()
        .filter(|i| i.health.is_routable())
        .map(|i| i.endpoint.clone())
        .collect()
}

/// Identify models that should be deleted due to idle (no requests in window).
///
/// Returns (instance_endpoint, model_name) pairs.
pub fn idle_models_for_deletion(
    instances: &HashMap<String, OllamaInstance>,
    model_request_counts: &HashMap<String, u64>,
    config: &RouterConfig,
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
