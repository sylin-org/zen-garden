//! Bootstrap task — monitors instances and manages replica set lifecycle.
//!
//! Watches for new MongoDB instances and:
//! 1. Groups by FQN
//! 2. For each FQN, checks if replica set exists (rs.status())
//! 3. If not initialized → rs.initiate()
//! 4. If initialized and new member not in set → rs.add()
//! 5. Updates replica set state
//! 6. Publishes connection string

use crate::app_state::AppState;
use crate::domain::bootstrap as bs;
use crate::domain::types::*;
use crate::infra::mongo_client::MongoClient;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How often to check for bootstrap actions (seconds).
const BOOTSTRAP_INTERVAL_SECS: u64 = 15;

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

        let rs_state = state.replica_set_for(fqn).await;
        let rs_name = derive_replica_set_name(fqn);

        // Try to probe current state from any reachable instance
        let probed_state = probe_replica_set(&instances, &rs_name).await;

        // If we got a successful probe, update state
        if let Some(ref probed) = probed_state {
            state.update_replica_set(fqn, probed.clone()).await;
        }

        let effective_state = probed_state.or(rs_state);

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
                        error = %e,
                        rs_name = %action.rs_name,
                        "replica set initiation failed"
                    );
                }
            }
            continue; // Let the newly initiated RS stabilize before adding members
        }

        // Check if we need to add members
        if let Some(ref rs) = effective_state {
            let add_actions = bs::should_add_members(&instances, rs);
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
                            error = %e,
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
                });
            }
            Err(e) => {
                // NotYetInitialized is expected for fresh instances
                let err_str = e.to_string();
                if err_str.contains("NotYetInitialized")
                    || err_str.contains("no replset config")
                {
                    return Some(ReplicaSetState {
                        rs_name: rs_name.to_string(),
                        initialized: false,
                        members: vec![],
                        connection_string: None,
                        last_updated: chrono::Utc::now(),
                    });
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
    })
}

/// Add a member to the replica set via the primary.
async fn add_member(primary_endpoint: &str, new_member_endpoint: &str) -> anyhow::Result<()> {
    let client = MongoClient::connect(primary_endpoint).await?;
    client.rs_add(new_member_endpoint).await
}
