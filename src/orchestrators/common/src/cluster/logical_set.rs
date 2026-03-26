//! Logical set — FQN-keyed group of instances forming one cluster.
//!
//! Generalizes MongoDB's `GroupState` + `GroupPhase` into a database-agnostic
//! lifecycle. Each adapter interprets phases via its `ClusterAdapter::probe()`
//! results; the generic layer tracks membership and phase transitions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::adapter::ProbeResult;

// ── Lifecycle Phase ───────────────────────────────────────────────────

/// Lifecycle phase of a logical set (one per FQN).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetPhase {
    /// No cluster exists. Instances need configuration + initialization.
    New,
    /// Configuration applied, waiting for instances to restart / join.
    Configuring,
    /// Cluster is operational.
    Healthy,
    /// Cluster exists but member endpoints have drifted (e.g. DHCP renewal).
    Drifted,
    /// Cluster is operational but degraded (members down, replication lag).
    Degraded,
}

// ── Membership Events ─────────────────────────────────────────────────

/// A change in set membership or health.
#[derive(Debug, Clone)]
pub enum MembershipEvent {
    /// A new instance joined the set.
    Added { endpoint: String, stone_name: String },
    /// An instance was removed from the set.
    Removed { endpoint: String },
    /// The set's lifecycle phase changed.
    PhaseChanged { from: SetPhase, to: SetPhase },
}

// ── Known Member ──────────────────────────────────────────────────────

/// A persisted member record for drift recovery.
///
/// When instances restart with new IPs (DHCP), the orchestrator uses
/// known members to map old→new endpoints by stone name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMember {
    /// Human-readable stone name.
    pub stone_name: String,
    /// Service endpoint at time of last successful health check.
    pub endpoint: String,
    /// Database-specific member identifier (e.g. MongoDB `_id`, PostgreSQL slot name).
    /// Stored as string for generality; adapters parse as needed.
    #[serde(default)]
    pub member_id: String,
}

// ── Logical Set ───────────────────────────────────────────────────────

/// Persisted state for one logical set (one per FQN).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicalSet {
    /// Set name (derived from FQN by the adapter).
    pub name: String,
    /// Current lifecycle phase.
    pub phase: SetPhase,
    /// Last known members — used for drift mapping on restart.
    pub known_members: Vec<KnownMember>,
    /// When this state was last updated.
    pub last_updated: DateTime<Utc>,
}

impl LogicalSet {
    /// Create a new logical set in the `New` phase.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            phase: SetPhase::New,
            known_members: Vec::new(),
            last_updated: Utc::now(),
        }
    }

    /// Update a known member's endpoint (for drift recovery).
    pub fn update_member_endpoint(&mut self, stone_name: &str, new_endpoint: &str) {
        if let Some(member) = self
            .known_members
            .iter_mut()
            .find(|m| m.stone_name == stone_name)
        {
            member.endpoint = new_endpoint.to_string();
        }
        self.last_updated = Utc::now();
    }

    /// Add a known member (or update if stone_name already exists).
    pub fn upsert_member(&mut self, member: KnownMember) {
        if let Some(existing) = self
            .known_members
            .iter_mut()
            .find(|m| m.stone_name == member.stone_name)
        {
            existing.endpoint = member.endpoint;
            existing.member_id = member.member_id;
        } else {
            self.known_members.push(member);
        }
        self.last_updated = Utc::now();
    }

    /// Remove a known member by endpoint.
    pub fn remove_member(&mut self, endpoint: &str) {
        self.known_members.retain(|m| m.endpoint != endpoint);
        self.last_updated = Utc::now();
    }

    /// Transition the phase, returning a `MembershipEvent` if changed.
    pub fn set_phase(&mut self, new_phase: SetPhase) -> Option<MembershipEvent> {
        if self.phase == new_phase {
            return None;
        }
        let from = self.phase.clone();
        self.phase = new_phase.clone();
        self.last_updated = Utc::now();
        Some(MembershipEvent::PhaseChanged {
            from,
            to: new_phase,
        })
    }

    /// Compute old→new endpoint mapping for drift recovery.
    ///
    /// Given current instance endpoints (stone_name → endpoint), maps
    /// known members' old endpoints to new ones by matching stone names.
    pub fn compute_drift_mapping(
        &self,
        current_instances: &[(String, String)], // (stone_name, current_endpoint)
    ) -> HashMap<String, String> {
        let stone_to_new: HashMap<&str, &str> = current_instances
            .iter()
            .map(|(name, ep)| (name.as_str(), ep.as_str()))
            .collect();

        let mut old_to_new = HashMap::new();
        for member in &self.known_members {
            if let Some(new_ep) = stone_to_new.get(member.stone_name.as_str()) {
                if member.endpoint != *new_ep {
                    old_to_new.insert(member.endpoint.clone(), new_ep.to_string());
                }
            }
        }
        old_to_new
    }
}

// ── Group Classifier ──────────────────────────────────────────────────

/// Action determined by classifying probe results for a logical set.
///
/// This is the generic equivalent of MongoDB's `GroupAction`. The adapter
/// acts on these actions using its database-specific operations.
#[derive(Debug)]
pub enum SetAction {
    /// Cluster not yet initialized — bootstrap on the given endpoint.
    Bootstrap { endpoint: String, set_name: String },
    /// All instances report stale config — drift recovery needed.
    RecoverDrift {
        connect_to: String,
        set_name: String,
        desired_endpoints: Vec<String>,
    },
    /// Some instances need configuration before joining.
    WaitForConfig,
    /// Cluster is operational.
    Healthy,
    /// No instances reachable.
    Wait,
}

/// Classify probe results into a set action (pure domain logic).
///
/// Mirrors MongoDB's `classify_group()` but database-agnostic.
pub fn classify_probes(
    probes: &[(String, ProbeResult)],
    set_name: &str,
    min_members_to_bootstrap: usize,
) -> SetAction {
    if probes.is_empty() {
        return SetAction::Wait;
    }

    let mut active = Vec::new();
    let mut not_initialized = Vec::new();
    let mut stale_config = Vec::new();
    let mut config_pending = 0usize;

    for (endpoint, probe) in probes {
        match probe {
            ProbeResult::Active => active.push(endpoint.as_str()),
            ProbeResult::NotInitialized => not_initialized.push(endpoint.as_str()),
            ProbeResult::StaleConfig => stale_config.push(endpoint.as_str()),
            ProbeResult::ConfigPending => config_pending += 1,
            ProbeResult::Unreachable => {}
        }
    }

    if !active.is_empty() {
        return SetAction::Healthy;
    }

    if config_pending > 0 {
        return SetAction::WaitForConfig;
    }

    if !stale_config.is_empty() && not_initialized.is_empty() {
        let desired = probes
            .iter()
            .filter(|(_, p)| matches!(p, ProbeResult::StaleConfig))
            .map(|(ep, _)| ep.clone())
            .collect();
        return SetAction::RecoverDrift {
            connect_to: stale_config[0].to_string(),
            set_name: set_name.to_string(),
            desired_endpoints: desired,
        };
    }

    if !not_initialized.is_empty() && probes.len() >= min_members_to_bootstrap {
        return SetAction::Bootstrap {
            endpoint: not_initialized[0].to_string(),
            set_name: set_name.to_string(),
        };
    }

    SetAction::Wait
}

// ── Persistence ───────────────────────────────────────────────────────

/// Load all logical sets from disk.
pub async fn load_sets(data_dir: &str) -> HashMap<String, LogicalSet> {
    let path = std::path::Path::new(data_dir).join("cluster-sets.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Persist all logical sets to disk.
pub async fn save_sets(data_dir: &str, sets: &HashMap<String, LogicalSet>) {
    let path = std::path::Path::new(data_dir).join("cluster-sets.json");
    match serde_json::to_string_pretty(sets) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&path, json).await {
                tracing::warn!(error = %e, "failed to persist logical sets");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize logical sets");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_all_active_is_healthy() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::Active),
            ("10.0.0.2:5432".into(), ProbeResult::Active),
        ];
        assert!(matches!(classify_probes(&probes, "test", 2), SetAction::Healthy));
    }

    #[test]
    fn classify_one_active_is_healthy() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::Active),
            ("10.0.0.2:5432".into(), ProbeResult::Unreachable),
        ];
        assert!(matches!(classify_probes(&probes, "test", 2), SetAction::Healthy));
    }

    #[test]
    fn classify_all_stale_is_drift() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::StaleConfig),
            ("10.0.0.2:5432".into(), ProbeResult::StaleConfig),
        ];
        assert!(matches!(
            classify_probes(&probes, "test", 2),
            SetAction::RecoverDrift { .. }
        ));
    }

    #[test]
    fn classify_config_pending_waits() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::ConfigPending),
        ];
        assert!(matches!(
            classify_probes(&probes, "test", 2),
            SetAction::WaitForConfig
        ));
    }

    #[test]
    fn classify_two_not_initialized_bootstraps() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::NotInitialized),
            ("10.0.0.2:5432".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(
            classify_probes(&probes, "test", 2),
            SetAction::Bootstrap { .. }
        ));
    }

    #[test]
    fn classify_single_not_initialized_waits() {
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(classify_probes(&probes, "test", 2), SetAction::Wait));
    }

    #[test]
    fn classify_empty_waits() {
        assert!(matches!(classify_probes(&[], "test", 2), SetAction::Wait));
    }

    #[test]
    fn drift_mapping() {
        let mut set = LogicalSet::new("test-set");
        set.known_members = vec![
            KnownMember {
                stone_name: "stone-a".into(),
                endpoint: "10.0.0.1:5432".into(),
                member_id: "1".into(),
            },
            KnownMember {
                stone_name: "stone-b".into(),
                endpoint: "10.0.0.2:5432".into(),
                member_id: "2".into(),
            },
        ];

        let current = vec![
            ("stone-a".into(), "10.0.0.50:5432".into()),
            ("stone-b".into(), "10.0.0.51:5432".into()),
        ];

        let mapping = set.compute_drift_mapping(&current);
        assert_eq!(mapping.len(), 2);
        assert_eq!(mapping["10.0.0.1:5432"], "10.0.0.50:5432");
        assert_eq!(mapping["10.0.0.2:5432"], "10.0.0.51:5432");
    }

    #[test]
    fn drift_mapping_no_change() {
        let mut set = LogicalSet::new("test");
        set.known_members = vec![KnownMember {
            stone_name: "stone-a".into(),
            endpoint: "10.0.0.1:5432".into(),
            member_id: "1".into(),
        }];

        let current = vec![("stone-a".into(), "10.0.0.1:5432".into())];
        assert!(set.compute_drift_mapping(&current).is_empty());
    }

    #[test]
    fn phase_transition_emits_event() {
        let mut set = LogicalSet::new("test");
        assert_eq!(set.phase, SetPhase::New);

        let event = set.set_phase(SetPhase::Healthy);
        assert!(event.is_some());
        assert_eq!(set.phase, SetPhase::Healthy);

        // Same phase → no event
        let event = set.set_phase(SetPhase::Healthy);
        assert!(event.is_none());
    }
}
