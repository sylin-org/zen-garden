//! Auto-tiering: compute VRAM tiers from discovered hardware.
//!
//! Tiers are emergent — computed from the set of distinct VRAM budgets
//! across all healthy instances. No predefined "small/medium/large" bins.

use super::types::{OllamaInstance, Tier};

const GIB: u64 = 1_073_741_824;

/// Compute tiers from a set of Ollama instances.
///
/// Groups instances by their VRAM budget, rounded to the nearest GiB.
/// Only healthy instances are included.
pub fn compute_tiers(instances: &[OllamaInstance]) -> Vec<Tier> {
    let mut groups: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();

    for inst in instances {
        if !inst.health.is_routable() {
            continue;
        }
        let tier_key = round_to_gib(inst.vram_budget_bytes);
        if tier_key == 0 {
            continue;
        }
        groups
            .entry(tier_key)
            .or_default()
            .push(inst.endpoint.clone());
    }

    groups
        .into_iter()
        .map(|(vram_bytes, instance_endpoints)| Tier {
            vram_bytes,
            label: format_vram_label(vram_bytes),
            instance_endpoints,
        })
        .collect()
}

/// Round a byte count to the nearest GiB (minimum 1 GiB).
fn round_to_gib(bytes: u64) -> u64 {
    if bytes == 0 {
        return 0;
    }
    let gib = (bytes as f64 / GIB as f64).round().max(1.0) as u64;
    gib * GIB
}

/// Human-readable tier label: "8G", "12G", "24G".
fn format_vram_label(bytes: u64) -> String {
    let gib = bytes / GIB;
    format!("{gib}G")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::InstanceHealth;
    use std::time::Instant;

    fn make_instance(endpoint: &str, vram_gb: u64, healthy: bool) -> OllamaInstance {
        OllamaInstance {
            stone_id: String::new(),
            stone_name: endpoint.to_string(),
            endpoint: endpoint.to_string(),
            moss_endpoint: None,
            ollama_version: None,
            gpu_name: None,
            vram_total_bytes: vram_gb * GIB,
            vram_budget_bytes: vram_gb * GIB,
            num_parallel: None,
            health: if healthy {
                InstanceHealth::Healthy
            } else {
                InstanceHealth::Unhealthy {
                    since: Instant::now(),
                    reason: "test".into(),
                }
            },
            models_loaded: vec![],
            models_available: vec![],
            queue_depth: 0,
            last_seen: Instant::now(),
            last_profiled: Instant::now(),
        }
    }

    #[test]
    fn single_tier() {
        let instances = vec![make_instance("a", 8, true), make_instance("b", 8, true)];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].label, "8G");
        assert_eq!(tiers[0].instance_endpoints.len(), 2);
    }

    #[test]
    fn multiple_tiers() {
        let instances = vec![
            make_instance("a", 8, true),
            make_instance("b", 12, true),
            make_instance("c", 24, true),
        ];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0].label, "8G");
        assert_eq!(tiers[1].label, "12G");
        assert_eq!(tiers[2].label, "24G");
    }

    #[test]
    fn unhealthy_excluded() {
        let instances = vec![make_instance("a", 8, true), make_instance("b", 24, false)];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].label, "8G");
    }
}
