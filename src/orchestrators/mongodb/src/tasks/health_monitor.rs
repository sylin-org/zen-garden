//! Health monitor task — periodic replica set health checking.
//!
//! Every 15 seconds, for each tracked replica set:
//! 1. Connect to any reachable member and run rs.status()
//! 2. Update instance health and roles in state
//! 3. Compute replication lag per secondary
//! 4. Query oplog info and evaluate oplog health
//! 5. Query serverStatus for WiredTiger cache metrics
//! 6. Detect membership changes and emit events
//! 7. Update connection strings
//! 8. Reconcile RS config if members don't match the logical set

use crate::app_state::AppState;
use crate::domain::cache_advisor;
use crate::domain::membership;
use crate::domain::oplog;
use crate::domain::types::*;
use crate::infra::mongo_client::MongoClient;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Health check interval.
const HEALTH_INTERVAL_SECS: u64 = 15;

/// Run the health monitor task.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Wait for initial discovery + bootstrap to populate state
    tokio::select! {
        _ = shutdown.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_secs(20)) => {}
    }

    let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("health monitor shutting down");
                return;
            }
            _ = interval.tick() => {
                health_cycle(&state).await;
            }
        }
    }
}

/// Run a single health check cycle across all replica sets.
async fn health_cycle(state: &AppState) {
    let fqns = state.distinct_fqns().await;

    for fqn in &fqns {
        let instances = state.instances_for_fqn(fqn).await;
        if instances.is_empty() {
            continue;
        }

        let old_rs_state = state.replica_set_for(fqn).await;

        // Try to probe from any reachable instance
        match probe_and_update(state, &instances, fqn).await {
            Some(new_rs_state) => {
                // Detect membership changes
                if let Some(ref old) = old_rs_state {
                    let changes = membership::detect_member_changes(old, &new_rs_state);
                    for change in &changes {
                        match change {
                            membership::MembershipEvent::RoleChanged {
                                stone_name,
                                old_role,
                                new_role,
                                ..
                            } => {
                                tracing::info!(
                                    stone = %stone_name,
                                    from = %old_role,
                                    to = %new_role,
                                    "role change detected"
                                );
                            }
                            membership::MembershipEvent::HealthChanged {
                                stone_name,
                                healthy,
                                ..
                            } => {
                                tracing::info!(
                                    stone = %stone_name,
                                    healthy = healthy,
                                    "member health change"
                                );
                            }
                            _ => {}
                        }
                    }

                    // Detect primary change
                    if let Some(pc) = membership::detect_primary_change(old, &new_rs_state) {
                        tracing::warn!(
                            old = ?pc.old_primary,
                            new = ?pc.new_primary,
                            "PRIMARY CHANGE detected"
                        );
                        state
                            .emit_event(
                                "rs.primary.changed",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "old": pc.old_primary,
                                    "new": pc.new_primary,
                                })
                                .to_string(),
                            )
                            .await;
                    }

                    if !changes.is_empty() {
                        state
                            .emit_event(
                                "rs.membership.changed",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "changes": changes.len(),
                                })
                                .to_string(),
                            )
                            .await;
                    }
                }

                state.update_replica_set(fqn, new_rs_state).await;
            }
            None => {
                tracing::debug!(fqn = %fqn, "no reachable instances for health check");
            }
        }
    }
}

/// Probe replica set status, update instance health/roles, compute lag,
/// query oplog and cache metrics.
///
/// If the RS member list doesn't match the logical set (registry endpoints),
/// triggers a one-shot `rs.reconfig` to rebuild the member list.
async fn probe_and_update(
    state: &AppState,
    instances: &[MongoInstance],
    fqn: &str,
) -> Option<ReplicaSetState> {
    let rs_name = derive_replica_set_name(fqn);

    // Active instances = non-Stopped (candidates for RS membership)
    let active_instances: Vec<&MongoInstance> = instances
        .iter()
        .filter(|i| i.health != InstanceHealth::Stopped)
        .collect();

    for instance in &active_instances {
        let client = match MongoClient::connect(&instance.mongo_endpoint).await {
            Ok(c) => c,
            Err(_) => {
                update_instance_health(state, &instance.mongo_endpoint, InstanceHealth::Unreachable)
                    .await;
                continue;
            }
        };

        // Ping first for quick check
        if !client.ping().await {
            update_instance_health(state, &instance.mongo_endpoint, InstanceHealth::Unreachable)
                .await;
            continue;
        }

        // Mark this instance as healthy
        update_instance_health(state, &instance.mongo_endpoint, InstanceHealth::Healthy).await;

        // Get rs.status()
        let status = match client.rs_status().await {
            Ok(s) => s,
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NotYetInitialized") || err_str.contains("no replset config") {
                    return Some(ReplicaSetState {
                        rs_name,
                        initialized: false,
                        members: vec![],
                        connection_string: None,
                        last_updated: chrono::Utc::now(),
                    });
                }
                tracing::debug!(error = %e, endpoint = %instance.mongo_endpoint, "rs.status() failed");
                continue;
            }
        };

        // ── Check if RS members match the logical set ──────────────
        // The logical set (registry) is the source of truth.
        // If RS has different endpoints, reconfig to match.
        let rs_hosts: HashSet<&str> = status.members.iter().map(|m| m.name.as_str()).collect();
        let registry_hosts: HashSet<&str> = active_instances
            .iter()
            .map(|i| i.mongo_endpoint.as_str())
            .collect();

        if rs_hosts != registry_hosts {
            tracing::warn!(
                fqn = %fqn,
                rs_members = ?rs_hosts,
                logical_set = ?registry_hosts,
                "RS members don't match logical set — rebuilding RS config"
            );

            // Find any reachable member to run reconfig against
            let desired: Vec<&str> = registry_hosts.into_iter().collect();
            // reconfig with force:true works from any member
            if let Err(e) = client.rs_reconfig_members(&rs_name, &desired).await {
                tracing::warn!(error = ?e, "RS reconfig failed — will retry next cycle");
            } else {
                tracing::info!(fqn = %fqn, members = ?desired, "RS config rebuilt to match logical set");
                state
                    .emit_event(
                        "rs.reconfig",
                        &serde_json::json!({ "fqn": fqn, "members": desired }).to_string(),
                    )
                    .await;
            }
            // Either way, return early — next cycle will see the new config
            return None;
        }

        // ── RS matches logical set — process normally ──────────────
        let primary_optime = status
            .members
            .iter()
            .find(|m| m.state == 1)
            .and_then(|m| m.optime_ts);

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

                let lag_seconds = if m.state == 2 {
                    match (primary_optime, m.optime_ts) {
                        (Some(primary), Some(secondary)) => {
                            Some((primary - secondary) as f64)
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                let stone_name = instances
                    .iter()
                    .find(|i| i.mongo_endpoint == m.name)
                    .map(|i| i.stone_name.clone())
                    .unwrap_or_else(|| m.name.clone());

                MemberState {
                    endpoint: m.name.clone(),
                    stone_name,
                    role,
                    healthy: m.health == 1.0,
                    lag_seconds,
                    last_heartbeat: m.last_heartbeat,
                }
            })
            .collect();

        // Update instance roles in state
        for member in &members {
            update_instance_role(state, &member.endpoint, member.role.clone()).await;
        }

        let conn_string = build_connection_string(&members, &status.set_name);

        // ── Oplog health (best-effort, only from primary) ──
        let max_lag = members
            .iter()
            .filter_map(|m| m.lag_seconds)
            .fold(0.0_f64, f64::max);

        if let Ok(repl_info) = client.replication_info().await {
            let oplog_health = oplog::evaluate_oplog(
                repl_info.oplog_window_secs,
                repl_info.oplog_used_mb,
                repl_info.oplog_size_mb,
                max_lag,
            );

            if oplog_health.severity != oplog::OplogSeverity::Healthy {
                tracing::warn!(
                    fqn = %fqn,
                    severity = %oplog_health.severity,
                    window = oplog_health.window_secs,
                    lag = oplog_health.max_lag_secs,
                    ratio = oplog_health.safety_ratio,
                    "oplog health concern"
                );
                state
                    .emit_event("oplog.health", &serde_json::to_string(&oplog_health).unwrap_or_default())
                    .await;
            }
        }

        // ── WiredTiger cache (best-effort) ──
        if let Ok(server_status) = client.server_status().await {
            if let Some(cache_status) = cache_advisor::parse_cache_status(&server_status) {
                let recs = cache_advisor::evaluate_cache(&cache_status, 0, 0);
                for rec in &recs {
                    if rec.severity != cache_advisor::CacheHealth::Healthy {
                        tracing::info!(
                            fqn = %fqn,
                            message = %rec.message,
                            "WiredTiger cache recommendation"
                        );
                    }
                }
            }
        }

        return Some(ReplicaSetState {
            rs_name: status.set_name,
            initialized: true,
            members,
            connection_string: Some(conn_string),
            last_updated: chrono::Utc::now(),
        });
    }

    None
}

/// Update a single instance's health in shared state.
async fn update_instance_health(state: &AppState, mongo_endpoint: &str, health: InstanceHealth) {
    let mut reg = state.instances.write().await;
    if let Some(inst) = reg.get_mut(mongo_endpoint) {
        inst.health = health;
        inst.last_seen = Instant::now();
    }
}

/// Update a single instance's role in shared state.
async fn update_instance_role(state: &AppState, mongo_endpoint: &str, role: ReplicaRole) {
    let mut reg = state.instances.write().await;
    if let Some(inst) = reg.get_mut(mongo_endpoint) {
        inst.role = Some(role);
    }
}
