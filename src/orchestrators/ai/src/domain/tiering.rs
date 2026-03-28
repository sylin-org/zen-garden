//! VRAM tier computation.
//!
//! Groups healthy service instances by VRAM budget (rounded to nearest GiB).
//! Used by routing for tier-aware selection and by the advisor for placement.
//!
//! Generalized from ollama-orchestrator domain/tiering.rs — operates on
//! `ServiceInstance` instead of `OllamaInstance`.

use std::collections::BTreeMap;

use super::types::{ServiceInstance, Tier};

/// Compute VRAM tiers from discovered instances.
///
/// Only healthy instances are included. Instances are grouped by their
/// VRAM budget rounded to the nearest GiB.
pub fn compute_tiers(instances: &[ServiceInstance]) -> Vec<Tier> {
    let mut tier_map: BTreeMap<u64, Vec<String>> = BTreeMap::new();

    for inst in instances {
        if !inst.health.is_healthy() {
            continue;
        }
        // CPU-only instances (zero VRAM budget) do not enter the tier map.
        if inst.vram.budget_bytes == 0 {
            continue;
        }
        let gib = round_to_gib(inst.vram.budget_bytes);
        tier_map
            .entry(gib)
            .or_default()
            .push(inst.endpoint.clone());
    }

    tier_map
        .into_iter()
        .map(|(gib, endpoints)| Tier {
            label: format_vram_label(gib),
            vram_bytes: gib * 1024 * 1024 * 1024,
            endpoints,
        })
        .collect()
}

/// Round bytes to the nearest GiB (minimum 1).
fn round_to_gib(bytes: u64) -> u64 {
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    (gib.round() as u64).max(1)
}

/// Human-readable tier label.
fn format_vram_label(gib: u64) -> String {
    format!("{gib}G")
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::*;
    use std::time::Instant;

    fn make_instance(endpoint: &str, vram_budget: u64, health: InstanceHealth) -> ServiceInstance {
        ServiceInstance {
            stone: Stone {
                id: "s1".into(),
                name: "stone-a".into(),
            },
            endpoint: endpoint.into(),
            kind: OfferingKind::Ollama,
            gpu: Gpu {
                name: Some("RTX 4090".into()),
                compute: ComputeType::Gpu,
            },
            vram: Vram {
                total_bytes: vram_budget,
                budget_bytes: vram_budget,
                free_bytes: None,
            },
            health,
            models_available: vec![],
            models_loaded: vec![],
            capabilities: vec![Capability::Chat],
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::Value::Null,
            priority: 0,
        }
    }

    fn unhealthy() -> InstanceHealth {
        InstanceHealth::Unhealthy {
            since: Instant::now(),
            reason: "test".into(),
        }
    }

    #[test]
    fn single_tier() {
        let instances = vec![
            make_instance("http://a:11434", 24 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
            make_instance("http://b:11434", 24 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
        ];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].label, "24G");
        assert_eq!(tiers[0].endpoints.len(), 2);
    }

    #[test]
    fn multiple_tiers() {
        let instances = vec![
            make_instance("http://a:11434", 8 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
            make_instance("http://b:11434", 24 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
        ];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].label, "8G");
        assert_eq!(tiers[1].label, "24G");
    }

    #[test]
    fn filters_unhealthy() {
        let instances = vec![
            make_instance("http://a:11434", 24 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
            make_instance("http://b:11434", 24 * 1024 * 1024 * 1024, unhealthy()),
        ];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].endpoints.len(), 1);
    }

    #[test]
    fn cpu_instance_excluded() {
        let instances = vec![
            make_instance("http://a:11434", 24 * 1024 * 1024 * 1024, InstanceHealth::Healthy),
            make_instance("http://b:11434", 0, InstanceHealth::Healthy), // CPU-only
        ];
        let tiers = compute_tiers(&instances);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].endpoints.len(), 1);
    }
}
