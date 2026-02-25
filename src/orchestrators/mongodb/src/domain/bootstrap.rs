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

    // Need at least one instance
    if instances.is_empty() {
        return None;
    }

    // Pick the first healthy instance (prefer instances that are actually reachable)
    let candidate = instances
        .iter()
        .find(|i| i.health == InstanceHealth::Healthy)
        .or_else(|| instances.first())?;

    let rs_name = derive_replica_set_name(fqn);

    Some(InitiateAction {
        endpoint: candidate.mongo_endpoint.clone(),
        rs_name,
    })
}

/// Determine whether a new member should be added to a replica set.
///
/// Returns `Some(AddMemberAction)` for each instance that is not yet in the
/// replica set's member list.
pub fn should_add_members(
    instances: &[MongoInstance],
    rs_state: &ReplicaSetState,
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

    instances
        .iter()
        .filter(|inst| !existing_endpoints.contains(inst.mongo_endpoint.as_str()))
        .map(|inst| AddMemberAction {
            primary_endpoint: primary.endpoint.clone(),
            new_member_endpoint: inst.mongo_endpoint.clone(),
            rs_name: rs_state.rs_name.clone(),
        })
        .collect()
}
