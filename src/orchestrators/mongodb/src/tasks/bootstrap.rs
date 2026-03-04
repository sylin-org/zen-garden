//! Bootstrap task — state machine for replica set lifecycle management.
//!
//! Each bootstrap cycle, for every FQN group:
//! 1. Ensure config patches (writes `--replSet` to mongod.conf)
//! 2. Probe each instance individually (per-instance classification)
//! 3. Classify the group state via pure domain logic
//! 4. Execute the determined action (initiate / reconfig / add / wait)
//! 5. Persist the updated group state to disk

use crate::app_state::AppState;
use crate::domain::group_state::{
    classify_group, compute_drift_mapping, GroupAction, GroupPhase, GroupState, InstanceProbe,
    KnownMember,
};
use crate::domain::types::*;
use crate::infra::mongo_client::MongoClient;
use garden_common::offerings::OfferingFqn;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How often to check for bootstrap actions (seconds).
const BOOTSTRAP_INTERVAL_SECS: u64 = 15;

/// The owner name for config patches applied by this orchestrator.
const CONFIG_PATCH_OWNER: &str = "mongodb-orchestrator";

/// Run the bootstrap task.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Wait a few seconds for initial discovery to populate instances
    tokio::select! {
        _ = shutdown.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_secs(10)) => {}
    }

    let mut interval = tokio::time::interval(Duration::from_secs(BOOTSTRAP_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("bootstrap task shutting down");
                return;
            }
            _ = interval.tick() => {
                if let Err(e) = bootstrap_cycle(&state).await {
                    tracing::warn!(error = %e, "bootstrap cycle failed");
                }
            }
        }
    }
}

/// Run a single bootstrap cycle: check all FQNs for needed actions.
async fn bootstrap_cycle(state: &AppState) -> anyhow::Result<()> {
    let fqns = state.distinct_fqns().await;
    if fqns.is_empty() {
        return Ok(());
    }

    for fqn in &fqns {
        let instances = state.instances_for_fqn(fqn).await;
        if instances.is_empty() {
            continue;
        }

        // Filter to manageable instances for config and probing.
        // Offline/Down stones are kept in the registry for dashboard display
        // but excluded from all RS management.  They re-enter when the tools
        // stream reports them back (OfferingDiscovered, ready=true).
        let active_instances: Vec<_> = instances
            .iter()
            .filter(|i| matches!(
                i.health,
                InstanceHealth::Unknown | InstanceHealth::Healthy | InstanceHealth::Degraded
            ))
            .cloned()
            .collect();

        let rs_name = derive_replica_set_name(fqn);

        // Step 1: Ensure active instances have the replica set config file patch
        if !active_instances.is_empty() {
            ensure_repl_set_config(&active_instances, &rs_name).await;
        }

        // Step 2: Probe each instance individually
        let probes = probe_instances(&active_instances).await;

        tracing::debug!(
            fqn = %fqn,
            probes = ?probes.iter().map(|(ep, p)| format!("{}={:?}", ep, p)).collect::<Vec<_>>(),
            "bootstrap probe results"
        );

        // Step 3: Classify — pure domain logic decides the action
        let action = classify_group(&probes, &rs_name);

        // Load persisted group state for drift mapping
        let group_state = state.group_for(fqn).await;

        // Step 4: Execute pending removals (if we have RS state from a previous cycle)
        let rs_state = state.replica_set_for(fqn).await;
        if let Some(ref rs) = rs_state {
            execute_pending_removals(state, rs, fqn).await;
        }

        // Step 5: Execute the classified action
        match action {
            GroupAction::Healthy => {
                // RS is operational — health monitor handles steady state.
                // Only log on phase transition; steady state is silent.
                if group_state.as_ref().map(|g| &g.phase) != Some(&GroupPhase::Healthy) {
                    tracing::info!(fqn = %fqn, rs_name = %rs_name, "RS operational — delegating to health monitor");
                    if let Some(mut gs) = group_state {
                        gs.phase = GroupPhase::Healthy;
                        gs.last_updated = chrono::Utc::now();
                        state.update_group(fqn, gs).await;
                    }
                }
            }

            GroupAction::WaitForConfig => {
                tracing::info!(
                    fqn = %fqn,
                    rs_name = %rs_name,
                    "waiting — config file patch pending on one or more instances"
                );
                // Update phase
                let gs = group_state.unwrap_or_else(|| GroupState {
                    rs_name: rs_name.clone(),
                    phase: GroupPhase::Configuring,
                    known_members: vec![],
                    last_updated: chrono::Utc::now(),
                });
                state
                    .update_group(
                        fqn,
                        GroupState {
                            phase: GroupPhase::Configuring,
                            last_updated: chrono::Utc::now(),
                            ..gs
                        },
                    )
                    .await;
            }

            GroupAction::Initiate { endpoint, rs_name } => {
                tracing::info!(
                    rs_name = %rs_name,
                    endpoint = %endpoint,
                    "initiating replica set"
                );

                match initiate_replica_set(&endpoint, &rs_name).await {
                    Ok(new_state) => {
                        tracing::info!(
                            rs_name = %rs_name,
                            "replica set initiated successfully"
                        );
                        // Persist group state with the new members
                        let known = new_state
                            .members
                            .iter()
                            .map(|m| KnownMember {
                                stone_name: m.stone_name.clone(),
                                endpoint: m.endpoint.clone(),
                                member_id: -1, // Will be updated by health monitor
                            })
                            .collect();
                        state
                            .update_group(
                                fqn,
                                GroupState {
                                    rs_name: new_state.rs_name.clone(),
                                    phase: GroupPhase::Healthy,
                                    known_members: known,
                                    last_updated: chrono::Utc::now(),
                                },
                            )
                            .await;
                        state.update_replica_set(fqn, new_state).await;
                        state
                            .emit_event(
                                "rs.initiated",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "rs_name": rs_name,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = ?e,
                            rs_name = %rs_name,
                            "replica set initiation failed"
                        );
                    }
                }
                continue; // Let the newly initiated RS stabilize before adding members
            }

            GroupAction::ReconfigDrift {
                connect_to,
                rs_name,
                desired,
            } => {
                tracing::warn!(
                    rs_name = %rs_name,
                    connect_to = %connect_to,
                    desired = ?desired,
                    "IP drift detected — all nodes report stale config, reconfiguring"
                );

                match execute_reconfig_drift(
                    &connect_to,
                    &rs_name,
                    &desired,
                    &active_instances,
                    group_state.as_ref(),
                )
                .await
                {
                    Ok(()) => {
                        // Build new known members from the desired list
                        let known: Vec<KnownMember> = desired
                            .iter()
                            .map(|ep| {
                                let stone_name = active_instances
                                    .iter()
                                    .find(|i| i.mongo_endpoint == *ep)
                                    .map(|i| i.stone_name.clone())
                                    .unwrap_or_else(|| ep.clone());
                                KnownMember {
                                    stone_name,
                                    endpoint: ep.clone(),
                                    member_id: -1, // Will be updated next health cycle
                                }
                            })
                            .collect();

                        state
                            .update_group(
                                fqn,
                                GroupState {
                                    rs_name: rs_name.clone(),
                                    phase: GroupPhase::Healthy,
                                    known_members: known,
                                    last_updated: chrono::Utc::now(),
                                },
                            )
                            .await;

                        state
                            .emit_event(
                                "rs.reconfig",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "reason": "ip_drift",
                                    "members": desired,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                    Err(e) => {
                        // Persist IpDrift phase so we remember the situation across restarts
                        state
                            .update_group(
                                fqn,
                                GroupState {
                                    rs_name: rs_name.clone(),
                                    phase: GroupPhase::IpDrift,
                                    known_members: group_state
                                        .map(|g| g.known_members)
                                        .unwrap_or_default(),
                                    last_updated: chrono::Utc::now(),
                                },
                            )
                            .await;

                        tracing::warn!(
                            error = ?e,
                            rs_name = %rs_name,
                            "IP drift reconfig failed — will retry next cycle"
                        );
                    }
                }
            }

            GroupAction::Wait => {
                tracing::info!(fqn = %fqn, rs_name = %rs_name, "all instances unreachable — waiting");
            }
        }

        // Membership management (add/remove) is handled solely by the
        // health monitor's reachability-based reconciliation.  Bootstrap
        // only owns lifecycle transitions (initiate, IP drift, config).
    }

    Ok(())
}

/// Execute pending removal actions for a specific FQN.
///
/// For each pending RemoveMember action matching this FQN: find the PRIMARY,
/// run rs.remove(), then clean up the instance and action from state.
async fn execute_pending_removals(
    state: &AppState,
    rs_state: &ReplicaSetState,
    fqn: &OfferingFqn,
) {
    if !rs_state.initialized {
        return;
    }

    let primary = match rs_state
        .members
        .iter()
        .find(|m| m.role == ReplicaRole::Primary)
    {
        Some(p) => p,
        None => return, // No primary — can't remove members
    };

    let actions = state.pending_actions_snapshot().await;
    for action in &actions {
        let (mongo_endpoint, action_fqn) = match action {
            PendingAction::RemoveMember {
                mongo_endpoint,
                fqn: action_fqn,
                ..
            } => (mongo_endpoint.as_str(), action_fqn),
        };

        if action_fqn != fqn {
            continue;
        }

        // Only attempt rs.remove() if the member is actually in the replica set
        let in_rs = rs_state
            .members
            .iter()
            .any(|m| m.endpoint == mongo_endpoint);

        if in_rs {
            tracing::info!(
                fqn = %fqn,
                member = %mongo_endpoint,
                primary = %primary.endpoint,
                "executing pending rs.remove()"
            );

            match remove_member(&primary.endpoint, mongo_endpoint).await {
                Ok(()) => {
                    tracing::info!(
                        member = %mongo_endpoint,
                        "member removed from replica set"
                    );
                    state
                        .emit_event(
                            "rs.member.removed",
                            &serde_json::json!({
                                "fqn": fqn,
                                "member": mongo_endpoint,
                            })
                            .to_string(),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::warn!(
                        error = ?e,
                        member = %mongo_endpoint,
                        "failed to remove member from replica set (will retry)"
                    );
                    continue; // Don't clean up — retry next cycle
                }
            }
        }

        // Member is either not in the RS or was successfully removed — clean up
        state.complete_action(mongo_endpoint).await;
        // Resolve mongo_endpoint to stone_name for instance registry removal
        if let Some(stone_name) = state.resolve_endpoint(mongo_endpoint).await {
            state.remove_instance(&stone_name).await;
        }
    }
}

// ============================================================================
// Config patch management
// ============================================================================

/// Ensure each instance has the replica set config file patch via Moss API.
///
/// Checks the config endpoint first (idempotent). If the patch isn't present,
/// applies it. Moss writes the config file and restarts the container.
async fn ensure_repl_set_config(instances: &[MongoInstance], rs_name: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    for instance in instances {
        if let Err(e) = apply_repl_set_patch(&client, instance, rs_name).await {
            tracing::debug!(
                stone = %instance.stone_name,
                error = ?e,
                "could not verify/apply repl set config patch"
            );
        }
    }
}

/// Check if the config patch exists; if not, apply it.
async fn apply_repl_set_patch(
    client: &reqwest::Client,
    instance: &MongoInstance,
    rs_name: &str,
) -> anyhow::Result<()> {
    let service_name = extract_service_name(&instance.fqn);
    let base_url = instance.moss_endpoint.trim_end_matches('/');
    let encoded_name = urlencoding::encode(&service_name);
    let config_url = format!(
        "{}/api/v1/stone/services/{}/config",
        base_url, encoded_name
    );

    // Check if patch already exists
    let check_url = format!("{}?owner={}", config_url, CONFIG_PATCH_OWNER);
    let resp = client.get(&check_url).send().await?;

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        let patches = body
            .get("patches")
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        if patches > 0 {
            // Patch already exists — nothing to do
            return Ok(());
        }
    }

    // Apply the config file patch — writes mongod.conf and restarts (no container recreation)
    tracing::info!(
        stone = %instance.stone_name,
        service = %service_name,
        rs_name = %rs_name,
        "applying replica set config file patch"
    );

    let mongod_conf = format!(
        "# Managed by mongodb-orchestrator\n\
         replication:\n  \
           replSetName: {}\n\
         net:\n  \
           bindIpAll: true\n",
        rs_name
    );

    let patch_body = serde_json::json!({
        "owner": CONFIG_PATCH_OWNER,
        "description": format!("Replica set configuration for {} pool", rs_name),
        "config": {
            "/etc/mongod.conf": mongod_conf,
        },
    });

    let resp = client
        .patch(&config_url)
        .json(&patch_body)
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::info!(
            stone = %instance.stone_name,
            service = %service_name,
            "config file patch applied — Moss will restart the container"
        );
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            stone = %instance.stone_name,
            status = %status,
            body = %body,
            "config patch request failed"
        );
    }

    Ok(())
}

/// Extract the service name from an FQN for use in Moss API paths.
///
/// Returns the canonical FQN string (e.g. `mongodb::prod`). Callers must
/// URL-encode this when embedding in path segments (the `::` separator is
/// not path-safe).
fn extract_service_name(fqn: &OfferingFqn) -> String {
    fqn.fqn()
}

// ============================================================================
// Instance probing
// ============================================================================

/// Probe each instance individually and return per-instance classification.
///
/// Unlike the old `probe_replica_set()` which returned the first success,
/// this probes every instance and returns a result for each — giving the
/// classifier the full picture.
async fn probe_instances(
    instances: &[MongoInstance],
) -> Vec<(String, InstanceProbe)> {
    let mut results = Vec::with_capacity(instances.len());

    for instance in instances {
        let client = match MongoClient::connect(&instance.mongo_endpoint).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    endpoint = %instance.mongo_endpoint,
                    error = %e,
                    "could not connect to MongoDB instance"
                );
                results.push((instance.mongo_endpoint.clone(), InstanceProbe::Unreachable));
                continue;
            }
        };

        match client.rs_status().await {
            Ok(_status) => {
                results.push((instance.mongo_endpoint.clone(), InstanceProbe::Active));
            }
            Err(e) => {
                // Use {:#} to include the full anyhow error chain —
                // e.to_string() only shows the .with_context() wrapper,
                // not the underlying MongoDB error code/name.
                let err_str = format!("{:#}", e);

                if err_str.contains("NotYetInitialized")
                    || err_str.contains("no replset config")
                {
                    results.push((
                        instance.mongo_endpoint.clone(),
                        InstanceProbe::NotInitialized,
                    ));
                } else if err_str.contains("NoReplicationEnabled")
                    || err_str.contains("error 76")
                {
                    tracing::debug!(
                        endpoint = %instance.mongo_endpoint,
                        "instance not configured for replication (config file patch pending)"
                    );
                    results
                        .push((instance.mongo_endpoint.clone(), InstanceProbe::ConfigPending));
                } else if err_str.contains("InvalidReplicaSetConfig")
                    || err_str.contains("error 93")
                {
                    tracing::debug!(
                        endpoint = %instance.mongo_endpoint,
                        "instance has stale RS config (IP drift)"
                    );
                    results
                        .push((instance.mongo_endpoint.clone(), InstanceProbe::StaleConfig));
                } else {
                    tracing::debug!(
                        endpoint = %instance.mongo_endpoint,
                        error = %e,
                        "rs.status() returned unrecognized error"
                    );
                    results
                        .push((instance.mongo_endpoint.clone(), InstanceProbe::Unreachable));
                }
            }
        }
    }

    results
}

// ============================================================================
// Replica set operations
// ============================================================================

/// Execute a force-reconfig to fix IP drift.
///
/// 1. Connect to any reachable instance
/// 2. Read `replSetGetConfig` (works even with error 93)
/// 3. Compute old→new IP mapping using persisted known members
/// 4. Call `rs_reconfig_members_with_mapping` with `force: true`
async fn execute_reconfig_drift(
    connect_to: &str,
    rs_name: &str,
    desired: &[String],
    active_instances: &[MongoInstance],
    group_state: Option<&GroupState>,
) -> anyhow::Result<()> {
    let client = MongoClient::connect(connect_to).await?;

    // Read the current (stale) RS config to get member _ids
    let (_config_rs_name, config_members) = client.rs_config().await?;

    // Build old→new mapping for _id preservation
    let old_to_new = if let Some(gs) = group_state {
        // Use persisted known members for stone_name→endpoint resolution
        let current: Vec<(String, String)> = active_instances
            .iter()
            .map(|i| (i.stone_name.clone(), i.mongo_endpoint.clone()))
            .collect();
        let rs_members: Vec<(i32, String)> = config_members
            .iter()
            .map(|m| (m.id, m.host.clone()))
            .collect();
        compute_drift_mapping(&rs_members, &current, &gs.known_members)
    } else {
        // No persisted state — can't map by stone name, reconfig will assign new _ids
        std::collections::HashMap::new()
    };

    if !old_to_new.is_empty() {
        tracing::info!(
            rs_name = %rs_name,
            mapping = ?old_to_new,
            "applying IP drift reconfig with _id preservation"
        );
    }

    let desired_refs: Vec<&str> = desired.iter().map(|s| s.as_str()).collect();
    client
        .rs_reconfig_members_with_mapping(rs_name, &desired_refs, &old_to_new)
        .await?;

    tracing::info!(
        rs_name = %rs_name,
        members = ?desired,
        "RS config updated — IP drift resolved"
    );

    Ok(())
}

/// Initiate a replica set on a single member.
async fn initiate_replica_set(
    endpoint: &str,
    rs_name: &str,
) -> anyhow::Result<ReplicaSetState> {
    let client = MongoClient::connect(endpoint).await?;
    let freshly_initiated = client.rs_initiate(rs_name).await?;

    if freshly_initiated {
        // Wait a moment for election, then probe status
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    let status = client.rs_status().await?;

    let members: Vec<MemberState> = status
        .members
        .iter()
        .map(|m| MemberState {
            endpoint: m.name.clone(),
            stone_name: m.name.clone(),
            role: if m.state == 1 {
                ReplicaRole::Primary
            } else {
                ReplicaRole::Secondary
            },
            healthy: m.health == 1.0,
            lag_seconds: None,
            last_heartbeat: m.last_heartbeat,
        })
        .collect();

    let conn = build_connection_string(&members, &status.set_name);

    Ok(ReplicaSetState {
        rs_name: status.set_name,
        initialized: true,
        members,
        connection_string: Some(conn),
        last_updated: chrono::Utc::now(),
        cache: None,
        oplog: None,
    })
}

/// Remove a member from the replica set via the primary.
async fn remove_member(primary_endpoint: &str, member_endpoint: &str) -> anyhow::Result<()> {
    let client = MongoClient::connect(primary_endpoint).await?;
    client.rs_remove(member_endpoint).await
}
