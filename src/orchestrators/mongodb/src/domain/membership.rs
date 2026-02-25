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
