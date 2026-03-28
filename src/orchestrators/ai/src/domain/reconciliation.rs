//! Registry drift detection.
//!
//! Compares the router's cached model registry against fresh probe data
//! to detect changes (models appeared/disappeared, VRAM changes, load state).
//!
//! Generalized from ollama-orchestrator domain/reconciliation.rs — operates
//! on `ServiceInstance` and `LoadedModel` instead of Ollama-specific types.

use std::collections::HashSet;

use super::types::LoadedModel;

/// A detected difference between cached and fresh instance state.
///
/// Every variant carries the `endpoint` of the instance that produced the
/// drift so callers can correlate without external bookkeeping.
#[derive(Debug, Clone)]
pub enum RegistryDrift {
    /// A model appeared on the instance (not previously known).
    ModelAppeared { endpoint: String, model: String },
    /// A model was removed from the instance.
    ModelDisappeared { endpoint: String, model: String },
    /// A model was loaded into VRAM.
    ModelLoaded {
        endpoint: String,
        model: String,
        vram_bytes: u64,
    },
    /// A model's VRAM consumption changed (different quant or reload).
    VramChanged {
        endpoint: String,
        model: String,
        old_vram: u64,
        new_vram: u64,
    },
    /// A model was unloaded from VRAM (evicted or manual).
    ModelUnloaded { endpoint: String, model: String },
}

/// Diff cached instance state against fresh probe data.
///
/// Returns a list of drifts. Empty list means no changes detected.
pub fn diff_instance(
    endpoint: &str,
    cached_available: &[String],
    cached_loaded: &[LoadedModel],
    fresh_available: &[String],
    fresh_loaded: &[LoadedModel],
) -> Vec<RegistryDrift> {
    let mut drifts = Vec::new();

    let old_set: HashSet<&str> = cached_available.iter().map(|s| s.as_str()).collect();
    let new_set: HashSet<&str> = fresh_available.iter().map(|s| s.as_str()).collect();

    // Models that appeared
    for &model in new_set.difference(&old_set) {
        drifts.push(RegistryDrift::ModelAppeared {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        });
    }

    // Models that disappeared
    for &model in old_set.difference(&new_set) {
        drifts.push(RegistryDrift::ModelDisappeared {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        });
    }

    // Load state changes
    let old_loaded: std::collections::HashMap<&str, u64> = cached_loaded
        .iter()
        .map(|m| (m.name.as_str(), m.vram_bytes))
        .collect();
    let new_loaded: std::collections::HashMap<&str, u64> = fresh_loaded
        .iter()
        .map(|m| (m.name.as_str(), m.vram_bytes))
        .collect();

    for (&model, &new_vram) in &new_loaded {
        match old_loaded.get(model) {
            None => {
                drifts.push(RegistryDrift::ModelLoaded {
                    endpoint: endpoint.to_string(),
                    model: model.to_string(),
                    vram_bytes: new_vram,
                });
            }
            Some(&old_vram) if old_vram != new_vram => {
                drifts.push(RegistryDrift::VramChanged {
                    endpoint: endpoint.to_string(),
                    model: model.to_string(),
                    old_vram,
                    new_vram,
                });
            }
            _ => {}
        }
    }

    for &model in old_loaded.keys() {
        if !new_loaded.contains_key(model) {
            drifts.push(RegistryDrift::ModelUnloaded {
                endpoint: endpoint.to_string(),
                model: model.to_string(),
            });
        }
    }

    drifts
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EP: &str = "http://stone-a:11434";

    #[test]
    fn no_drift() {
        let avail = vec!["llama3:8b".to_string()];
        let loaded = vec![LoadedModel {
            name: "llama3:8b".into(),
            vram_bytes: 4_000_000_000,
            expires_at: None,
        }];
        let drifts = diff_instance(EP, &avail, &loaded, &avail, &loaded);
        assert!(drifts.is_empty());
    }

    #[test]
    fn model_appeared() {
        let old = vec!["llama3:8b".to_string()];
        let new = vec!["llama3:8b".to_string(), "qwen:7b".to_string()];
        let drifts = diff_instance(EP, &old, &[], &new, &[]);
        assert!(drifts.iter().any(|d| matches!(d, RegistryDrift::ModelAppeared { model, .. } if model == "qwen:7b")));
    }

    #[test]
    fn model_disappeared() {
        let old = vec!["llama3:8b".to_string(), "qwen:7b".to_string()];
        let new = vec!["llama3:8b".to_string()];
        let drifts = diff_instance(EP, &old, &[], &new, &[]);
        assert!(drifts.iter().any(|d| matches!(d, RegistryDrift::ModelDisappeared { model, .. } if model == "qwen:7b")));
    }

    #[test]
    fn model_loaded() {
        let avail = vec!["llama3:8b".to_string()];
        let old_loaded: Vec<LoadedModel> = vec![];
        let new_loaded = vec![LoadedModel {
            name: "llama3:8b".into(),
            vram_bytes: 4_000_000_000,
            expires_at: None,
        }];
        let drifts = diff_instance(EP, &avail, &old_loaded, &avail, &new_loaded);
        assert!(drifts.iter().any(|d| matches!(d, RegistryDrift::ModelLoaded { model, .. } if model == "llama3:8b")));
    }

    #[test]
    fn model_unloaded() {
        let avail = vec!["llama3:8b".to_string()];
        let old_loaded = vec![LoadedModel {
            name: "llama3:8b".into(),
            vram_bytes: 4_000_000_000,
            expires_at: None,
        }];
        let new_loaded: Vec<LoadedModel> = vec![];
        let drifts = diff_instance(EP, &avail, &old_loaded, &avail, &new_loaded);
        assert!(drifts.iter().any(|d| matches!(d, RegistryDrift::ModelUnloaded { model, .. } if model == "llama3:8b")));
    }

    #[test]
    fn vram_changed() {
        let avail = vec!["llama3:8b".to_string()];
        let old_loaded = vec![LoadedModel {
            name: "llama3:8b".into(),
            vram_bytes: 4_000_000_000,
            expires_at: None,
        }];
        let new_loaded = vec![LoadedModel {
            name: "llama3:8b".into(),
            vram_bytes: 5_000_000_000,
            expires_at: None,
        }];
        let drifts = diff_instance(EP, &avail, &old_loaded, &avail, &new_loaded);
        assert!(drifts.iter().any(|d| matches!(d, RegistryDrift::VramChanged { model, .. } if model == "llama3:8b")));
    }
}
