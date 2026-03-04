//! Group state machine for MongoDB replica set lifecycle.
//!
//! Each FQN (e.g. `mongodb`, `mongodb::prod`) has a group of instances that
//! form one logical replica set. This module tracks the lifecycle phase of
//! each group and classifies the appropriate action based on per-instance
//! probe results.
//!
//! The group state is persisted to disk so the orchestrator can resume with
//! knowledge of the last known RS membership — critical for computing
//! old→new IP drift mappings after DHCP renewal.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ============================================================================
// Per-instance probe result
// ============================================================================

/// Result of probing a single MongoDB instance's replica set status.
#[derive(Debug)]
pub enum InstanceProbe {
    /// `rs.status()` succeeded — RS initialized, this node is a member.
    Active,
    /// `NotYetInitialized` — started with `--replSet` but no RS exists yet.
    NotInitialized,
    /// `InvalidReplicaSetConfig` (error 93) — RS exists but this node's IP
    /// is not in the current config (typically DHCP drift).
    StaleConfig,
    /// `NoReplicationEnabled` (error 76) — not started with `--replSet`.
    /// The config patch hasn't been applied / container not yet restarted.
    ConfigPending,
    /// Connection failed or unrecognized error.
    Unreachable,
}

// ============================================================================
// Group lifecycle
// ============================================================================

/// Lifecycle phase of a replica set group (one per FQN).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupPhase {
    /// No RS exists. Instances need config patches + initiation.
    New,
    /// Config patches applied, waiting for container restarts.
    Configuring,
    /// RS exists, operating normally.
    Healthy,
    /// RS exists but member IPs have drifted (all instances report error 93).
    IpDrift,
}

/// Persisted state for one FQN group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupState {
    /// Replica set name (e.g. `"zen-garden"`, `"zen-garden-prod"`).
    pub rs_name: String,
    /// Current lifecycle phase.
    pub phase: GroupPhase,
    /// Last known RS members — used for drift mapping on restart.
    pub known_members: Vec<KnownMember>,
    /// When this state was last updated.
    pub last_updated: DateTime<Utc>,
}

/// A known RS member with its MongoDB `_id` for reconfig preservation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMember {
    /// Human-readable stone name.
    pub stone_name: String,
    /// MongoDB wire endpoint (e.g. `"192.168.1.175:27017"`).
    pub endpoint: String,
    /// MongoDB member `_id` (stable across reconfigs).
    pub member_id: i32,
}

// ============================================================================
// Classifier output
// ============================================================================

/// Action determined by the group classifier.
#[derive(Debug)]
pub enum GroupAction {
    /// RS not yet initiated — initiate on this endpoint.
    Initiate {
        endpoint: String,
        rs_name: String,
    },
    /// All nodes report error 93 — force-reconfig with IP drift mapping.
    ReconfigDrift {
        /// Any reachable endpoint to connect to for `replSetGetConfig` + reconfig.
        connect_to: String,
        rs_name: String,
        /// Current endpoints (new IPs) for the desired member list.
        desired: Vec<String>,
    },
    /// Some instances still need `--replSet` config patch + restart.
    WaitForConfig,
    /// RS is healthy — health monitor handles steady state.
    Healthy,
    /// All instances unreachable — can't do anything.
    Wait,
}

// ============================================================================
// Classifier (pure domain logic)
// ============================================================================

/// Classify the group's current state and determine the appropriate action.
///
/// Takes per-instance probe results and the persisted group state.
/// Returns the action the bootstrap should execute.
pub fn classify_group(
    probes: &[(String, InstanceProbe)], // (mongo_endpoint, probe result)
    rs_name: &str,
) -> GroupAction {
    if probes.is_empty() {
        return GroupAction::Wait;
    }

    let mut active_endpoints = Vec::new();
    let mut not_initialized = Vec::new();
    let mut stale_config = Vec::new();
    let mut config_pending = 0usize;
    for (endpoint, probe) in probes {
        match probe {
            InstanceProbe::Active => active_endpoints.push(endpoint.as_str()),
            InstanceProbe::NotInitialized => not_initialized.push(endpoint.as_str()),
            InstanceProbe::StaleConfig => stale_config.push(endpoint.as_str()),
            InstanceProbe::ConfigPending => config_pending += 1,
            InstanceProbe::Unreachable => {}
        }
    }

    // If ANY instance returned a healthy rs.status(), the RS is operational.
    // Health monitor handles steady-state management from here.
    if !active_endpoints.is_empty() {
        return GroupAction::Healthy;
    }

    // If any instances still need --replSet config, wait for restart.
    // This takes priority over StaleConfig — mixed states mean some
    // nodes haven't been restarted yet.
    if config_pending > 0 {
        return GroupAction::WaitForConfig;
    }

    // All reachable instances report error 93 — RS exists but IPs drifted.
    if !stale_config.is_empty() && not_initialized.is_empty() {
        let desired: Vec<String> = probes
            .iter()
            .filter(|(_, p)| matches!(p, InstanceProbe::StaleConfig))
            .map(|(ep, _)| ep.clone())
            .collect();

        return GroupAction::ReconfigDrift {
            connect_to: stale_config[0].to_string(),
            rs_name: rs_name.to_string(),
            desired,
        };
    }

    // Any instances report NotYetInitialized — RS needs to be created.
    // This covers the "all NotInitialized" case as well as the mixed
    // NotInitialized + StaleConfig case (some nodes were wiped, some
    // still have stale RS data). Initiate on a clean node; stale nodes
    // will be added after the RS stabilizes.
    if !not_initialized.is_empty() {
        return GroupAction::Initiate {
            endpoint: not_initialized[0].to_string(),
            rs_name: rs_name.to_string(),
        };
    }

    // All unreachable
    GroupAction::Wait
}

/// Compute old→new endpoint mapping for drift reconfig.
///
/// Given the current `rs.conf()` members (old IPs) and the current instance
/// endpoints (new IPs), maps old→new by matching through stone names in the
/// known members list.
pub fn compute_drift_mapping(
    rs_config_members: &[(i32, String)], // (member_id, old_endpoint)
    current_instances: &[(String, String)], // (stone_name, current_endpoint)
    known_members: &[KnownMember],
) -> HashMap<String, String> {
    let mut old_to_new = HashMap::new();

    // Build: old_endpoint → stone_name from known members
    let old_ep_to_stone: HashMap<&str, &str> = known_members
        .iter()
        .map(|km| (km.endpoint.as_str(), km.stone_name.as_str()))
        .collect();

    // Build: stone_name → current_endpoint from current instances
    let stone_to_new: HashMap<&str, &str> = current_instances
        .iter()
        .map(|(name, ep)| (name.as_str(), ep.as_str()))
        .collect();

    for (_id, old_ep) in rs_config_members {
        if let Some(stone_name) = old_ep_to_stone.get(old_ep.as_str()) {
            if let Some(new_ep) = stone_to_new.get(stone_name) {
                if old_ep != *new_ep {
                    old_to_new.insert(old_ep.clone(), new_ep.to_string());
                }
            }
        }
    }

    old_to_new
}

// ============================================================================
// Persistence
// ============================================================================

/// Load all group states from disk.
pub async fn load_groups(data_dir: &str) -> HashMap<String, GroupState> {
    let path = Path::new(data_dir).join("groups.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Persist all group states to disk.
pub async fn save_groups(data_dir: &str, groups: &HashMap<String, GroupState>) {
    let path = Path::new(data_dir).join("groups.json");
    match serde_json::to_string_pretty(groups) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&path, json).await {
                tracing::warn!(error = %e, "failed to persist group states");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize group states");
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_all_active_is_healthy() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::Active),
            ("192.168.1.2:27017".into(), InstanceProbe::Active),
        ];
        assert!(matches!(
            classify_group(&probes, "zen-garden"),
            GroupAction::Healthy
        ));
    }

    #[test]
    fn classify_one_active_is_healthy() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::Active),
            ("192.168.1.2:27017".into(), InstanceProbe::Unreachable),
        ];
        assert!(matches!(
            classify_group(&probes, "zen-garden"),
            GroupAction::Healthy
        ));
    }

    #[test]
    fn classify_all_stale_is_reconfig_drift() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::StaleConfig),
            ("192.168.1.2:27017".into(), InstanceProbe::StaleConfig),
        ];
        match classify_group(&probes, "zen-garden") {
            GroupAction::ReconfigDrift {
                connect_to,
                desired,
                ..
            } => {
                assert_eq!(connect_to, "192.168.1.1:27017");
                assert_eq!(desired.len(), 2);
            }
            other => panic!("expected ReconfigDrift, got {:?}", other),
        }
    }

    #[test]
    fn classify_all_config_pending_is_wait() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::ConfigPending),
            ("192.168.1.2:27017".into(), InstanceProbe::ConfigPending),
        ];
        assert!(matches!(
            classify_group(&probes, "zen-garden"),
            GroupAction::WaitForConfig
        ));
    }

    #[test]
    fn classify_mixed_stale_and_config_pending_waits() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::StaleConfig),
            ("192.168.1.2:27017".into(), InstanceProbe::ConfigPending),
        ];
        assert!(matches!(
            classify_group(&probes, "zen-garden"),
            GroupAction::WaitForConfig
        ));
    }

    #[test]
    fn classify_all_not_initialized_is_initiate() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::NotInitialized),
        ];
        match classify_group(&probes, "zen-garden") {
            GroupAction::Initiate { endpoint, rs_name } => {
                assert_eq!(endpoint, "192.168.1.1:27017");
                assert_eq!(rs_name, "zen-garden");
            }
            other => panic!("expected Initiate, got {:?}", other),
        }
    }

    #[test]
    fn classify_all_unreachable_is_wait() {
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::Unreachable),
        ];
        assert!(matches!(
            classify_group(&probes, "zen-garden"),
            GroupAction::Wait
        ));
    }

    #[test]
    fn classify_empty_is_wait() {
        assert!(matches!(
            classify_group(&[], "zen-garden"),
            GroupAction::Wait
        ));
    }

    #[test]
    fn classify_mixed_stale_and_not_initialized_initiates() {
        // One node was wiped (NotInitialized), the other still has stale RS data.
        // Should initiate on the clean node.
        let probes = vec![
            ("192.168.1.1:27017".into(), InstanceProbe::NotInitialized),
            ("192.168.1.2:27017".into(), InstanceProbe::StaleConfig),
        ];
        match classify_group(&probes, "zen-garden") {
            GroupAction::Initiate { endpoint, .. } => {
                assert_eq!(endpoint, "192.168.1.1:27017");
            }
            other => panic!("expected Initiate, got {:?}", other),
        }
    }

    #[test]
    fn drift_mapping_computes_old_to_new() {
        let rs_members = vec![
            (11, "192.168.1.175:27017".into()),
            (12, "192.168.1.182:27017".into()),
        ];
        let current = vec![
            ("stone-a".into(), "192.168.1.168:27017".into()),
            ("stone-b".into(), "192.168.1.174:27017".into()),
        ];
        let known = vec![
            KnownMember {
                stone_name: "stone-a".into(),
                endpoint: "192.168.1.175:27017".into(),
                member_id: 11,
            },
            KnownMember {
                stone_name: "stone-b".into(),
                endpoint: "192.168.1.182:27017".into(),
                member_id: 12,
            },
        ];

        let mapping = compute_drift_mapping(&rs_members, &current, &known);
        assert_eq!(mapping.len(), 2);
        assert_eq!(
            mapping.get("192.168.1.175:27017"),
            Some(&"192.168.1.168:27017".to_string())
        );
        assert_eq!(
            mapping.get("192.168.1.182:27017"),
            Some(&"192.168.1.174:27017".to_string())
        );
    }

    #[test]
    fn drift_mapping_no_change_returns_empty() {
        let rs_members = vec![(11, "192.168.1.168:27017".into())];
        let current = vec![("stone-a".into(), "192.168.1.168:27017".into())];
        let known = vec![KnownMember {
            stone_name: "stone-a".into(),
            endpoint: "192.168.1.168:27017".into(),
            member_id: 11,
        }];

        let mapping = compute_drift_mapping(&rs_members, &current, &known);
        assert!(mapping.is_empty());
    }
}
