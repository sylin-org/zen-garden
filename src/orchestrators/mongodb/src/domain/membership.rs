//! Membership management — detect changes between old and new RS states.

use super::types::*;
use std::time::Duration;

/// Decision about whether to remove a member.
#[derive(Debug)]
pub enum RemovalDecision {
    /// Member is healthy, keep it.
    Keep,
    /// Member has been unreachable too long, recommend removal.
    RecommendRemoval { reason: String },
}

/// A detected change in replica set membership.
#[derive(Debug, Clone)]
pub enum MembershipEvent {
    /// A new member appeared in rs.status().
    MemberAdded {
        endpoint: String,
        stone_name: String,
        role: ReplicaRole,
    },
    /// A member disappeared from rs.status().
    MemberRemoved {
        endpoint: String,
        stone_name: String,
    },
    /// A member's role changed (e.g. election happened).
    RoleChanged {
        endpoint: String,
        stone_name: String,
        old_role: ReplicaRole,
        new_role: ReplicaRole,
    },
    /// A member's health changed.
    HealthChanged {
        endpoint: String,
        stone_name: String,
        healthy: bool,
    },
}

/// A primary change event.
#[derive(Debug, Clone)]
pub struct PrimaryChangeEvent {
    pub old_primary: Option<String>,
    pub new_primary: Option<String>,
}

/// Evaluate whether a member should be removed based on its health timeout.
pub fn evaluate_removal(member: &MemberState, timeout: Duration) -> RemovalDecision {
    if member.healthy {
        return RemovalDecision::Keep;
    }

    // Check last heartbeat age
    if let Some(last_hb) = member.last_heartbeat {
        let age = chrono::Utc::now() - last_hb;
        if age.num_seconds() > timeout.as_secs() as i64 {
            return RemovalDecision::RecommendRemoval {
                reason: format!(
                    "unreachable for {}s (timeout: {}s)",
                    age.num_seconds(),
                    timeout.as_secs()
                ),
            };
        }
    }

    RemovalDecision::Keep
}

/// Detect primary change between old and new RS state.
pub fn detect_primary_change(
    old: &ReplicaSetState,
    new: &ReplicaSetState,
) -> Option<PrimaryChangeEvent> {
    let old_primary = old
        .members
        .iter()
        .find(|m| m.role == ReplicaRole::Primary)
        .map(|m| m.endpoint.clone());

    let new_primary = new
        .members
        .iter()
        .find(|m| m.role == ReplicaRole::Primary)
        .map(|m| m.endpoint.clone());

    if old_primary != new_primary {
        Some(PrimaryChangeEvent {
            old_primary,
            new_primary,
        })
    } else {
        None
    }
}

/// Detect all membership changes between old and new RS state.
pub fn detect_member_changes(
    old: &ReplicaSetState,
    new: &ReplicaSetState,
) -> Vec<MembershipEvent> {
    let mut events = Vec::new();

    let old_endpoints: std::collections::HashSet<&str> =
        old.members.iter().map(|m| m.endpoint.as_str()).collect();
    let new_endpoints: std::collections::HashSet<&str> =
        new.members.iter().map(|m| m.endpoint.as_str()).collect();

    // Detect additions
    for member in &new.members {
        if !old_endpoints.contains(member.endpoint.as_str()) {
            events.push(MembershipEvent::MemberAdded {
                endpoint: member.endpoint.clone(),
                stone_name: member.stone_name.clone(),
                role: member.role.clone(),
            });
        }
    }

    // Detect removals
    for member in &old.members {
        if !new_endpoints.contains(member.endpoint.as_str()) {
            events.push(MembershipEvent::MemberRemoved {
                endpoint: member.endpoint.clone(),
                stone_name: member.stone_name.clone(),
            });
        }
    }

    // Detect role and health changes for existing members
    for new_member in &new.members {
        if let Some(old_member) = old
            .members
            .iter()
            .find(|m| m.endpoint == new_member.endpoint)
        {
            if old_member.role != new_member.role {
                events.push(MembershipEvent::RoleChanged {
                    endpoint: new_member.endpoint.clone(),
                    stone_name: new_member.stone_name.clone(),
                    old_role: old_member.role.clone(),
                    new_role: new_member.role.clone(),
                });
            }

            if old_member.healthy != new_member.healthy {
                events.push(MembershipEvent::HealthChanged {
                    endpoint: new_member.endpoint.clone(),
                    stone_name: new_member.stone_name.clone(),
                    healthy: new_member.healthy,
                });
            }
        }
    }

    events
}

/// Result of quorum loss detection.
#[derive(Debug)]
pub struct QuorumLossDetected {
    /// Endpoints of healthy members that should remain in the RS.
    pub healthy_endpoints: Vec<String>,
    /// Endpoints of DOWN/unhealthy members being evicted.
    pub evicted_endpoints: Vec<String>,
    /// Total member count before eviction.
    pub total_members: usize,
}

/// Detect quorum loss: RS is initialized, no PRIMARY exists, at least one
/// healthy SECONDARY, and at least one DOWN/unhealthy member.
///
/// Returns `Some` when a force-reconfig should be issued to restore writes.
/// Returns `None` when quorum is intact, an election is in progress, or
/// there are no healthy members to reconfig through.
pub fn detect_quorum_loss(rs: &ReplicaSetState) -> Option<QuorumLossDetected> {
    if !rs.initialized || rs.members.is_empty() {
        return None;
    }

    // If a PRIMARY exists, quorum is intact
    if rs.members.iter().any(|m| m.role == ReplicaRole::Primary) {
        return None;
    }

    let healthy: Vec<&MemberState> = rs
        .members
        .iter()
        .filter(|m| m.healthy && m.role == ReplicaRole::Secondary)
        .collect();

    let unhealthy: Vec<&MemberState> = rs
        .members
        .iter()
        .filter(|m| !m.healthy || matches!(m.role, ReplicaRole::Down | ReplicaRole::Removed))
        .collect();

    // Need both: someone to reconfig through AND someone to evict
    if healthy.is_empty() || unhealthy.is_empty() {
        return None;
    }

    Some(QuorumLossDetected {
        healthy_endpoints: healthy.iter().map(|m| m.endpoint.clone()).collect(),
        evicted_endpoints: unhealthy.iter().map(|m| m.endpoint.clone()).collect(),
        total_members: rs.members.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(endpoint: &str, role: ReplicaRole, healthy: bool) -> MemberState {
        MemberState {
            endpoint: endpoint.to_string(),
            stone_name: endpoint.to_string(),
            role,
            healthy,
            lag_seconds: None,
            last_heartbeat: None,
        }
    }

    fn rs(members: Vec<MemberState>) -> ReplicaSetState {
        ReplicaSetState {
            rs_name: "zen-garden".to_string(),
            initialized: true,
            members,
            connection_string: None,
            last_updated: chrono::Utc::now(),
            cache: None,
            oplog: None,
        }
    }

    #[test]
    fn quorum_loss_2_member_one_down() {
        let state = rs(vec![
            member("a:27017", ReplicaRole::Secondary, true),
            member("b:27017", ReplicaRole::Down, false),
        ]);
        let result = detect_quorum_loss(&state).unwrap();
        assert_eq!(result.healthy_endpoints, vec!["a:27017"]);
        assert_eq!(result.evicted_endpoints, vec!["b:27017"]);
        assert_eq!(result.total_members, 2);
    }

    #[test]
    fn quorum_loss_3_member_two_down() {
        let state = rs(vec![
            member("a:27017", ReplicaRole::Secondary, true),
            member("b:27017", ReplicaRole::Down, false),
            member("c:27017", ReplicaRole::Down, false),
        ]);
        let result = detect_quorum_loss(&state).unwrap();
        assert_eq!(result.healthy_endpoints, vec!["a:27017"]);
        assert_eq!(result.evicted_endpoints.len(), 2);
    }

    #[test]
    fn no_quorum_loss_when_primary_exists() {
        let state = rs(vec![
            member("a:27017", ReplicaRole::Primary, true),
            member("b:27017", ReplicaRole::Down, false),
        ]);
        assert!(detect_quorum_loss(&state).is_none());
    }

    #[test]
    fn no_quorum_loss_election_in_progress() {
        // Both healthy secondaries, no primary yet — let MongoDB finish electing
        let state = rs(vec![
            member("a:27017", ReplicaRole::Secondary, true),
            member("b:27017", ReplicaRole::Secondary, true),
        ]);
        assert!(detect_quorum_loss(&state).is_none());
    }

    #[test]
    fn no_quorum_loss_all_down() {
        // No one to reconfig through
        let state = rs(vec![
            member("a:27017", ReplicaRole::Down, false),
            member("b:27017", ReplicaRole::Down, false),
        ]);
        assert!(detect_quorum_loss(&state).is_none());
    }

    #[test]
    fn no_quorum_loss_not_initialized() {
        let mut state = rs(vec![]);
        state.initialized = false;
        assert!(detect_quorum_loss(&state).is_none());
    }
}
