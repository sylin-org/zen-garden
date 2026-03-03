//! Bootstrap task — monitors instances and manages replica set lifecycle.
//!
//! Watches for new MongoDB instances and:
//! 1. Ensures each instance has the replica set config file patch applied
//!    (writes mongod.conf via Moss config API → Moss restarts container)
//! 2. Groups by FQN
//! 3. For each FQN, checks if replica set exists (rs.status())
//! 4. If not initialized → rs.initiate()
//! 5. If initialized and new member not in set → rs.add()
//! 6. Updates replica set state
//! 7. Publishes connection string

use crate::app_state::AppState;
use crate::domain::bootstrap as bs;
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

    // Collect pending removal endpoints once per cycle
    let pending_removal_endpoints: Vec<String> = state
        .pending_actions_snapshot()
        .await
        .iter()
        .map(|a| a.target_endpoint().to_string())
        .collect();

    for fqn in &fqns {
        let instances = state.instances_for_fqn(fqn).await;
        if instances.is_empty() {
            continue;
        }

        // Filter to non-stopped instances for config and probing
        let active_instances: Vec<_> = instances
            .iter()
            .filter(|i| i.health != InstanceHealth::Stopped)
            .cloned()
            .collect();

        let rs_name = derive_replica_set_name(fqn);

        // Step 1: Ensure active instances have the replica set config file patch
        if !active_instances.is_empty() {
            ensure_repl_set_config(&active_instances, &rs_name).await;
        }

        let rs_state = state.replica_set_for(fqn).await;

        // Try to probe current state from any reachable instance
        let probed_state = probe_replica_set(&active_instances, &rs_name).await;

        // If we got a successful probe, update state
        if let Some(ref probed) = probed_state {
            state.update_replica_set(fqn, probed.clone()).await;
        }

        let effective_state = probed_state.or(rs_state);

        // Step 2: Execute pending removal actions
        if let Some(ref rs) = effective_state {
            execute_pending_removals(state, rs, fqn).await;
        }

        // Check if we need to initiate
        if let Some(action) = bs::should_initiate(&instances, &effective_state, fqn) {
            tracing::info!(
                rs_name = %action.rs_name,
                endpoint = %action.endpoint,
                "initiating replica set"
            );

            match initiate_replica_set(&action.endpoint, &action.rs_name).await {
                Ok(new_state) => {
                    tracing::info!(
                        rs_name = %action.rs_name,
                        "replica set initiated successfully"
                    );
                    state.update_replica_set(fqn, new_state).await;
                    state
                        .emit_event(
                            "rs.initiated",
                            &serde_json::json!({
                                "fqn": fqn,
                                "rs_name": action.rs_name,
                            })
                            .to_string(),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::warn!(
                        error = ?e,
                        rs_name = %action.rs_name,
                        "replica set initiation failed"
                    );
                }
            }
            continue; // Let the newly initiated RS stabilize before adding members
        }

        // Check if we need to add members
        if let Some(ref rs) = effective_state {
            let add_actions =
                bs::should_add_members(&instances, rs, &pending_removal_endpoints);
            for action in add_actions {
                tracing::info!(
                    rs_name = %action.rs_name,
                    new_member = %action.new_member_endpoint,
                    primary = %action.primary_endpoint,
                    "adding member to replica set"
                );

                match add_member(&action.primary_endpoint, &action.new_member_endpoint).await {
                    Ok(()) => {
                        tracing::info!(
                            member = %action.new_member_endpoint,
                            "member added successfully"
                        );
                        state
                            .emit_event(
                                "rs.member.added",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "member": action.new_member_endpoint,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = ?e,
                            member = %action.new_member_endpoint,
                            "failed to add member"
                        );
                    }
                }
            }
        }
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
// Replica set operations
// ============================================================================

/// Probe the replica set status from any reachable instance.
async fn probe_replica_set(
    instances: &[MongoInstance],
    rs_name: &str,
) -> Option<ReplicaSetState> {
    for instance in instances {
        let client = match MongoClient::connect(&instance.mongo_endpoint).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        match client.rs_status().await {
            Ok(status) => {
                let members: Vec<MemberState> = status
                    .members
                    .iter()
                    .map(|m| {
                        let role = match m.state {
                            1 => ReplicaRole::Primary,
                            2 => ReplicaRole::Secondary,
                            7 => ReplicaRole::Arbiter,
                            3 | 5 => ReplicaRole::Recovering,
                            0 | 6 => ReplicaRole::Startup,
                            _ => ReplicaRole::Unknown,
                        };

                        MemberState {
                            endpoint: m.name.clone(),
                            stone_name: instances
                                .iter()
                                .find(|i| i.mongo_endpoint == m.name)
                                .map(|i| i.stone_name.clone())
                                .unwrap_or_else(|| m.name.clone()),
                            role,
                            healthy: m.health == 1.0,
                            lag_seconds: None, // Computed separately by health monitor
                            last_heartbeat: m.last_heartbeat,
                        }
                    })
                    .collect();

                let conn_string = if !members.is_empty() {
                    Some(build_connection_string(&members, &status.set_name))
                } else {
                    None
                };

                return Some(ReplicaSetState {
                    rs_name: status.set_name,
                    initialized: true,
                    members,
                    connection_string: conn_string,
                    last_updated: chrono::Utc::now(),
                    cache: None,
                    oplog: None,
                });
            }
            Err(e) => {
                let err_str = e.to_string();

                // NotYetInitialized is expected for fresh instances started with --replSet
                if err_str.contains("NotYetInitialized")
                    || err_str.contains("no replset config")
                {
                    return Some(ReplicaSetState {
                        rs_name: rs_name.to_string(),
                        initialized: false,
                        members: vec![],
                        connection_string: None,
                        last_updated: chrono::Utc::now(),
                        cache: None,
                        oplog: None,
                    });
                }

                // NoReplicationEnabled (error 76) means the instance doesn't have
                // replication enabled in its config. The ensure_repl_set_config step
                // should write the config file and restart on the next cycle.
                if err_str.contains("NoReplicationEnabled") || err_str.contains("error 76") {
                    tracing::debug!(
                        endpoint = %instance.mongo_endpoint,
                        "instance not configured for replication (config file patch pending)"
                    );
                    continue;
                }

                tracing::debug!(
                    endpoint = %instance.mongo_endpoint,
                    error = %e,
                    "could not probe rs.status()"
                );
                continue;
            }
        }
    }

    None
}

/// Initiate a replica set on a single member.
async fn initiate_replica_set(
    endpoint: &str,
    rs_name: &str,
) -> anyhow::Result<ReplicaSetState> {
    let client = MongoClient::connect(endpoint).await?;
    client.rs_initiate(rs_name).await?;

    // Wait a moment for election, then probe status
    tokio::time::sleep(Duration::from_secs(3)).await;

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

/// Add a member to the replica set via the primary.
async fn add_member(primary_endpoint: &str, new_member_endpoint: &str) -> anyhow::Result<()> {
    let client = MongoClient::connect(primary_endpoint).await?;
    client.rs_add(new_member_endpoint).await
}

/// Remove a member from the replica set via the primary.
async fn remove_member(primary_endpoint: &str, member_endpoint: &str) -> anyhow::Result<()> {
    let client = MongoClient::connect(primary_endpoint).await?;
    client.rs_remove(member_endpoint).await
}
