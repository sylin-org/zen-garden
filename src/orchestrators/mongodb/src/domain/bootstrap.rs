//! Bootstrap logic for MongoDB replica sets.
//!
//! Pure domain logic — determines *what* actions to take based on current
//! state. The actual execution (rs.initiate, rs.add) is done by the
//! bootstrap task in `tasks/bootstrap.rs`.

use super::types::*;

/// Action: initialize a new replica set on a single member.
#[derive(Debug)]
pub struct InitiateAction {
    /// Endpoint to initiate on (the lone member).
    pub endpoint: String,
    /// Replica set name to use.
    pub rs_name: String,
}

/// Action: add a new member to an existing replica set.
#[derive(Debug)]
pub struct AddMemberAction {
    /// Endpoint of the PRIMARY to send rs.add() to.
    pub primary_endpoint: String,
    /// Endpoint of the new member to add.
    pub new_member_endpoint: String,
    /// Replica set name.
    pub rs_name: String,
}

/// Determine whether a replica set should be initiated.
///
/// Returns `Some(InitiateAction)` if there is exactly one instance for this
/// FQN and the replica set has not been initialized yet.
pub fn should_initiate(
    instances: &[MongoInstance],
    rs_state: &Option<ReplicaSetState>,
    fqn: &str,
) -> Option<InitiateAction> {
    // Already initialized — nothing to do
    if rs_state.as_ref().is_some_and(|rs| rs.initialized) {
        return None;
    }

    // Need at least one non-stopped instance
    let active_instances: Vec<_> = instances
        .iter()
        .filter(|i| i.health != InstanceHealth::Stopped)
        .collect();
    if active_instances.is_empty() {
        return None;
    }

    // Pick the first healthy instance (prefer instances that are actually reachable)
    let candidate = active_instances
        .iter()
        .find(|i| i.health == InstanceHealth::Healthy)
        .or_else(|| active_instances.first())
        .copied()?;

    let rs_name = derive_replica_set_name(fqn);

    Some(InitiateAction {
        endpoint: candidate.mongo_endpoint.clone(),
        rs_name,
    })
}

/// Determine whether a new member should be added to a replica set.
///
/// Returns `Some(AddMemberAction)` for each instance that is not yet in the
/// replica set's member list. Skips instances that are Stopped (container down)
/// or have a pending removal action.
///
/// Membership is checked by both endpoint AND stone name — this prevents
/// spurious rs.add() attempts when a stone's IP changes (DHCP renewal) but
/// the stone is already in the RS under its old IP.
pub fn should_add_members(
    instances: &[MongoInstance],
    rs_state: &ReplicaSetState,
    pending_removal_endpoints: &[String],
) -> Vec<AddMemberAction> {
    if !rs_state.initialized {
        return vec![];
    }

    // Find the primary
    let primary = match rs_state
        .members
        .iter()
        .find(|m| m.role == ReplicaRole::Primary)
    {
        Some(p) => p,
        None => return vec![], // No primary — can't add members
    };

    let existing_endpoints: std::collections::HashSet<&str> =
        rs_state.members.iter().map(|m| m.endpoint.as_str()).collect();

    // Also track existing stone names — a stone already in the RS under a
    // different IP (IP drift) should not be re-added as a new member.
    let existing_stone_names: std::collections::HashSet<&str> =
        rs_state.members.iter().map(|m| m.stone_name.as_str()).collect();

    instances
        .iter()
        .filter(|inst| {
            // Skip instances already in the replica set (by endpoint)
            if existing_endpoints.contains(inst.mongo_endpoint.as_str()) {
                return false;
            }
            // Skip instances already in the replica set (by stone name — IP may have changed)
            if existing_stone_names.contains(inst.stone_name.as_str()) {
                return false;
            }
            // Skip stopped instances (container down)
            if inst.health == InstanceHealth::Stopped {
                return false;
            }
            // Skip instances with pending removal
            if pending_removal_endpoints.contains(&inst.mongo_endpoint) {
                return false;
            }
            true
        })
        .map(|inst| AddMemberAction {
            primary_endpoint: primary.endpoint.clone(),
            new_member_endpoint: inst.mongo_endpoint.clone(),
            rs_name: rs_state.rs_name.clone(),
        })
        .collect()
}
