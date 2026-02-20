//! Registry reconciliation: detect drift between the router's model
//! registry and the actual state of Ollama instances.
//!
//! Pure logic — takes snapshots and produces diffs. The I/O (polling
//! instances) is in the reconciliation task.

use super::types::{LoadedModel, OllamaInstance};

/// A change detected during reconciliation.
#[derive(Debug, Clone)]
pub enum RegistryDrift {
    /// Model appeared on instance (pulled outside the router).
    ModelAppeared {
        instance_endpoint: String,
        model_name: String,
    },
    /// Model disappeared from instance (deleted outside the router).
    ModelDisappeared {
        instance_endpoint: String,
        model_name: String,
    },
    /// Model was loaded into VRAM.
    ModelLoaded {
        instance_endpoint: String,
        model_name: String,
        size_vram: u64,
    },
    /// Model was unloaded from VRAM (evicted or manual).
    ModelUnloaded {
        instance_endpoint: String,
        model_name: String,
    },
    /// VRAM usage changed (different quant or reloaded).
    VramChanged {
        instance_endpoint: String,
        model_name: String,
        old_vram: u64,
        new_vram: u64,
    },
}

/// Diff the router's registry against fresh data from an instance.
///
/// Returns a list of drifts that the caller should apply.
pub fn diff_instance(
    instance: &OllamaInstance,
    fresh_models_available: &[String],
    fresh_models_loaded: &[LoadedModel],
) -> Vec<RegistryDrift> {
    let mut drifts = Vec::new();
    let ep = &instance.endpoint;

    // Models that appeared (in fresh but not in registry)
    for model in fresh_models_available {
        if !instance.models_available.iter().any(|m| m == model) {
            drifts.push(RegistryDrift::ModelAppeared {
                instance_endpoint: ep.clone(),
                model_name: model.clone(),
            });
        }
    }

    // Models that disappeared (in registry but not in fresh)
    for model in &instance.models_available {
        if !fresh_models_available.iter().any(|m| m == model) {
            drifts.push(RegistryDrift::ModelDisappeared {
                instance_endpoint: ep.clone(),
                model_name: model.clone(),
            });
        }
    }

    // Load state changes
    let old_loaded: std::collections::HashMap<&str, u64> = instance
        .models_loaded
        .iter()
        .map(|m| (m.name.as_str(), m.size_vram))
        .collect();

    let new_loaded: std::collections::HashMap<&str, u64> = fresh_models_loaded
        .iter()
        .map(|m| (m.name.as_str(), m.size_vram))
        .collect();

    // Newly loaded
    for (name, &vram) in &new_loaded {
        if !old_loaded.contains_key(name) {
            drifts.push(RegistryDrift::ModelLoaded {
                instance_endpoint: ep.clone(),
                model_name: name.to_string(),
                size_vram: vram,
            });
        } else if let Some(&old_vram) = old_loaded.get(name) {
            if old_vram != vram {
                drifts.push(RegistryDrift::VramChanged {
                    instance_endpoint: ep.clone(),
                    model_name: name.to_string(),
                    old_vram,
                    new_vram: vram,
                });
            }
        }
    }

    // Unloaded
    for name in old_loaded.keys() {
        if !new_loaded.contains_key(name) {
            drifts.push(RegistryDrift::ModelUnloaded {
                instance_endpoint: ep.clone(),
                model_name: name.to_string(),
            });
        }
    }

    drifts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::InstanceHealth;
    use std::time::Instant;

    fn empty_instance(ep: &str, models: &[&str], loaded: Vec<LoadedModel>) -> OllamaInstance {
        OllamaInstance {
            stone_id: String::new(),
            stone_name: String::new(),
            endpoint: ep.to_string(),
            ollama_version: None,
            gpu_name: None,
            vram_total_bytes: 0,
            vram_budget_bytes: 0,
            health: InstanceHealth::Healthy,
            models_loaded: loaded,
            models_available: models.iter().map(|s| s.to_string()).collect(),
            queue_depth: 0,
            last_seen: Instant::now(),
            last_profiled: Instant::now(),
        }
    }

    #[test]
    fn detects_new_model() {
        let inst = empty_instance("ep", &["a"], vec![]);
        let drifts = diff_instance(&inst, &["a".into(), "b".into()], &[]);
        assert!(drifts.iter().any(
            |d| matches!(d, RegistryDrift::ModelAppeared { model_name, .. } if model_name == "b")
        ));
    }

    #[test]
    fn detects_removed_model() {
        let inst = empty_instance("ep", &["a", "b"], vec![]);
        let drifts = diff_instance(&inst, &["a".into()], &[]);
        assert!(drifts.iter().any(
            |d| matches!(d, RegistryDrift::ModelDisappeared { model_name, .. } if model_name == "b")
        ));
    }

    #[test]
    fn detects_load_change() {
        let loaded = vec![LoadedModel {
            name: "a".into(),
            size_vram: 1000,
            expires_at: None,
        }];
        let inst = empty_instance("ep", &["a"], loaded);
        // Model unloaded
        let drifts = diff_instance(&inst, &["a".into()], &[]);
        assert!(drifts
            .iter()
            .any(|d| matches!(d, RegistryDrift::ModelUnloaded { .. })));
    }
}
