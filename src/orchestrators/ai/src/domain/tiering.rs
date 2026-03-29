//! Auto-tiering: compute VRAM tiers from discovered hardware.
//!
//! Tiers are emergent — computed from the set of distinct VRAM budgets
//! across all healthy instances. No predefined "small/medium/large" bins.

use super::types::{ServiceInstance, Tier};

const GIB: u64 = 1_073_741_824;

/// Compute tiers from a set of service instances.
///
/// Groups instances by their VRAM budget, rounded to the nearest GiB.
/// Only healthy instances are included.
pub fn compute_tiers(instances: &HashMap<String, ServiceInstance>) -> Vec<Tier> {
    let mut groups: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();

    for inst in instances.values() {
        if !inst.is_routable() {
            continue;
        }
        let tier_key = round_to_gib(inst.vram.budget_bytes);
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

use std::collections::HashMap;

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
    use crate::domain::types::{ComputeType, Gpu, InstanceHealth, OfferingKind, Stone, Vram};
    use std::time::Instant;

    fn make_instance(endpoint: &str, vram_gb: u64, healthy: bool) -> ServiceInstance {
        ServiceInstance {
            stone: Stone { id: String::new(), name: endpoint.to_string() },
            endpoint: endpoint.to_string(),
            kind: OfferingKind::Ollama,
            gpu: Gpu { name: None, compute: ComputeType::Gpu },
            vram: Vram { total_bytes: vram_gb * GIB, budget_bytes: vram_gb * GIB, free_bytes: None },
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
            capabilities: vec![],
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::Value::Null,
            priority: 0,
        }
    }

    #[test]
    fn single_tier() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), make_instance("a", 8, true));
        instances.insert("b".into(), make_instance("b", 8, true));
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].label, "8G");
        assert_eq!(tiers[0].instance_endpoints.len(), 2);
    }

    #[test]
    fn multiple_tiers() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), make_instance("a", 8, true));
        instances.insert("b".into(), make_instance("b", 12, true));
        instances.insert("c".into(), make_instance("c", 24, true));
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0].label, "8G");
        assert_eq!(tiers[1].label, "12G");
        assert_eq!(tiers[2].label, "24G");
    }

    #[test]
    fn unhealthy_excluded() {
        let mut instances = HashMap::new();
        instances.insert("a".into(), make_instance("a", 8, true));
        instances.insert("b".into(), make_instance("b", 24, false));
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].label, "8G");
    }
}
