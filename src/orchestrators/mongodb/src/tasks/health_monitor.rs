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
use garden_common::offerings::OfferingFqn;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Health check interval.
const HEALTH_INTERVAL_SECS: u64 = 15;

/// Minimum interval between repeated log lines for the same severity.
const LOG_COOLDOWN: Duration = Duration::from_secs(300);

/// Run the health monitor task.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Wait for initial discovery + bootstrap to populate state
    tokio::select! {
        _ = shutdown.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_secs(20)) => {}
    }

    let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_INTERVAL_SECS));
    let mut cache_log_state: HashMap<OfferingFqn, (cache_advisor::CacheHealth, Instant)> =
        HashMap::new();
    let mut oplog_log_state: HashMap<OfferingFqn, (oplog::OplogSeverity, Instant)> = HashMap::new();

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("health monitor shutting down");
                return;
            }
            _ = interval.tick() => {
                health_cycle(&state, &mut cache_log_state, &mut oplog_log_state).await;
            }
        }
    }
}

/// Run a single health check cycle across all replica sets.
async fn health_cycle(
    state: &AppState,
    cache_log_state: &mut HashMap<OfferingFqn, (cache_advisor::CacheHealth, Instant)>,
    oplog_log_state: &mut HashMap<OfferingFqn, (oplog::OplogSeverity, Instant)>,
) {
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

                // ── Rate-limited cache logging + event emission ──
                if let Some(ref cache) = new_rs_state.cache {
                    let should_log = match cache_log_state.get(fqn) {
                        Some((prev_health, last_logged)) => {
                            *prev_health != cache.health
                                || last_logged.elapsed() >= LOG_COOLDOWN
                        }
                        None => cache.health != cache_advisor::CacheHealth::Healthy,
                    };
                    if should_log && cache.health != cache_advisor::CacheHealth::Healthy {
                        tracing::info!(
                            fqn = %fqn,
                            dirty_ratio = format_args!("{:.1}%", cache.status.dirty_ratio * 100.0),
                            hit_ratio = format_args!("{:.1}%", cache.status.hit_ratio * 100.0),
                            health = ?cache.health,
                            "WiredTiger cache pressure"
                        );
                    }
                    cache_log_state
                        .insert(fqn.clone(), (cache.health.clone(), Instant::now()));

                    state
                        .emit_event(
                            "cache.updated",
                            &serde_json::json!({
                                "fqn": fqn,
                                "health": cache.health,
                                "dirty_ratio": cache.status.dirty_ratio,
                                "hit_ratio": cache.status.hit_ratio,
                            })
                            .to_string(),
                        )
                        .await;
                }

                // ── Rate-limited oplog logging ──
                if let Some(ref oplog_health) = new_rs_state.oplog {
                    let should_log = match oplog_log_state.get(fqn) {
                        Some((prev_severity, last_logged)) => {
                            *prev_severity != oplog_health.severity
                                || last_logged.elapsed() >= LOG_COOLDOWN
                        }
                        None => oplog_health.severity != oplog::OplogSeverity::Healthy,
                    };
                    if should_log && oplog_health.severity != oplog::OplogSeverity::Healthy {
                        tracing::warn!(
                            fqn = %fqn,
                            severity = %oplog_health.severity,
                            window = oplog_health.window_secs,
                            lag = oplog_health.max_lag_secs,
                            "oplog health concern"
                        );
                    }
                    oplog_log_state
                        .insert(fqn.clone(), (oplog_health.severity.clone(), Instant::now()));

                    if oplog_health.severity != oplog::OplogSeverity::Healthy {
                        state
                            .emit_event(
                                "oplog.health",
                                &serde_json::to_string(&oplog_health).unwrap_or_default(),
                            )
                            .await;
                    }
                }

                // Persist known members for drift detection across restarts
                state.update_group_members(fqn, &new_rs_state).await;
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
/// Uses a two-phase approach to prevent oscillation when a stone is down:
/// 1. **Reachability sweep**: connect + ping ALL active instances to determine
///    which are actually reachable right now.
/// 2. **RS status + reconciliation**: compare RS membership against the
///    **reachable** set only (not all active instances).  This prevents the
///    loop where membership reconciliation re-adds an unreachable member
///    that quorum recovery just evicted.
async fn probe_and_update(
    state: &AppState,
    instances: &[MongoInstance],
    fqn: &OfferingFqn,
) -> Option<ReplicaSetState> {
    let rs_name = derive_replica_set_name(fqn);

    // Manageable instances = candidates for RS membership.
    // Offline/Down/Stopped are kept for dashboard display but excluded from
    // probing and reconciliation.  They re-enter when the tools stream
    // reports them back (OfferingDiscovered, ready=true → health = Unknown).
    let active_instances: Vec<&MongoInstance> = instances
        .iter()
        .filter(|i| matches!(
            i.health,
            InstanceHealth::Unknown | InstanceHealth::Healthy | InstanceHealth::Degraded
        ))
        .collect();

    // ── Phase 1: Reachability sweep ─────────────────────────────────
    // Probe ALL active instances (connect + ping) to get accurate
    // reachability before making any membership decisions.
    let mut reachable: Vec<(&MongoInstance, MongoClient)> = Vec::new();

    for instance in &active_instances {
        let client = match MongoClient::connect(&instance.mongo_endpoint).await {
            Ok(c) => c,
            Err(_) => {
                let health = classify_unreachable(&instance.moss_endpoint).await;
                update_instance_health(state, &instance.mongo_endpoint, health.clone()).await;
                if health == InstanceHealth::Offline {
                    flush_stale_endpoints(state, &instance.mongo_endpoint).await;
                }
                continue;
            }
        };

        if !client.ping().await {
            let health = classify_unreachable(&instance.moss_endpoint).await;
            update_instance_health(state, &instance.mongo_endpoint, health.clone()).await;
            if health == InstanceHealth::Offline {
                flush_stale_endpoints(state, &instance.mongo_endpoint).await;
            }
            continue;
        }

        update_instance_health(state, &instance.mongo_endpoint, InstanceHealth::Healthy).await;
        reachable.push((instance, client));
    }

    if reachable.is_empty() {
        return None;
    }

    // ── Phase 2: RS status + reconciliation ─────────────────────────
    // Build registry from REACHABLE instances only.  This prevents the
    // oscillation where membership reconciliation re-adds an unreachable
    // member that quorum recovery just evicted.
    let registry_hosts: HashSet<&str> = reachable
        .iter()
        .map(|(i, _)| i.mongo_endpoint.as_str())
        .collect();

    let registry_name_to_endpoint: HashMap<&str, &str> = reachable
        .iter()
        .map(|(i, _)| (i.stone_name.as_str(), i.mongo_endpoint.as_str()))
        .collect();

    let registry_stone_names: HashSet<&str> = registry_name_to_endpoint.keys().copied().collect();

    for (instance, client) in &reachable {
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
                        cache: None,
                        oplog: None,
                    });
                }
                tracing::debug!(error = %e, endpoint = %instance.mongo_endpoint, "rs.status() failed");
                continue;
            }
        };

        // ── Check if RS members match the reachable set ─────────
        // Two-tier comparison:
        // 1. Resolve RS member endpoints to stone names via catalog
        //    (handles IP changes: old IP in RS → catalog → stone name)
        // 2. Fallback to raw endpoint comparison for stones not yet in catalog
        //
        // When names match but endpoints differ (IP drift from DHCP),
        // we build an old→new endpoint mapping so rs_reconfig_members
        // can preserve MongoDB member _ids across the IP change.

        let rs_hosts: HashSet<&str> = status.members.iter().map(|m| m.name.as_str()).collect();

        // Build stone_name→RS_endpoint map from RS members
        let rs_name_to_endpoint: HashMap<String, String> = {
            let catalog = state.catalog.read().await;
            status
                .members
                .iter()
                .filter_map(|m| {
                    catalog
                        .resolve_name(m.name.as_str())
                        .map(|name| (name.to_string(), m.name.clone()))
                })
                .collect()
        };

        let rs_resolved_names: HashSet<&str> = rs_name_to_endpoint.keys().map(|s| s.as_str()).collect();

        // Determine match status
        let all_rs_resolved = rs_name_to_endpoint.len() == status.members.len();
        let names_match = all_rs_resolved && rs_resolved_names == registry_stone_names;
        let endpoints_match = rs_hosts == registry_hosts;

        if names_match && endpoints_match {
            // Perfect — same stones, same IPs. No action needed.
        } else if names_match && !endpoints_match {
            // IP drift: same stones but endpoints changed (DHCP renewal).
            // Build old→new mapping for _id-preserving reconfig.
            let mut drift_details = Vec::new();
            let mut old_to_new: HashMap<String, String> = HashMap::new();
            for (stone, old_ep) in &rs_name_to_endpoint {
                if let Some(new_ep) = registry_name_to_endpoint.get(stone.as_str()) {
                    if old_ep != *new_ep {
                        drift_details.push(format!(
                            "{}: {} → {}", stone, old_ep, new_ep
                        ));
                        old_to_new.insert(old_ep.clone(), new_ep.to_string());
                    }
                }
            }

            tracing::warn!(
                fqn = %fqn,
                changes = ?drift_details,
                "IP drift detected — same stones, different endpoints. Reconfiguring RS."
            );

            let desired: Vec<&str> = registry_hosts.iter().copied().collect();
            if let Err(e) = client
                .rs_reconfig_members_with_mapping(&rs_name, &desired, &old_to_new)
                .await
            {
                tracing::warn!(error = ?e, "RS reconfig (IP drift) failed — will retry next cycle");
            } else {
                tracing::info!(
                    fqn = %fqn,
                    members = ?desired,
                    "RS config updated for IP drift — member _ids preserved"
                );
                state
                    .emit_event(
                        "rs.reconfig",
                        &serde_json::json!({
                            "fqn": fqn,
                            "reason": "ip_drift",
                            "members": desired,
                            "changes": drift_details,
                        })
                        .to_string(),
                    )
                    .await;
            }
            return None;
        } else if !names_match {
            // Membership change: different stones (added/removed).
            tracing::warn!(
                fqn = %fqn,
                rs_members = ?rs_hosts,
                logical_set = ?registry_hosts,
                "RS members don't match logical set — rebuilding RS config"
            );

            let desired: Vec<&str> = registry_hosts.iter().copied().collect();
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
            return None;
        }

        // ── RS matches logical set — process normally ──────────────

        // Diagnostic dump at DEBUG level (enable with RUST_LOG=...=debug)
        tracing::debug!(
            fqn = %fqn,
            rs_members = ?status.members.iter().map(|m| format!("{}(state={},health={},str={})", m.name, m.state, m.health, m.state_str)).collect::<Vec<_>>(),
            "health cycle: rs state"
        );

        let primary_optime = status
            .members
            .iter()
            .find(|m| m.state == 1)
            .and_then(|m| m.optime_ts);

        // Read catalog once (outside the map closure which is sync)
        let member_catalog = state.catalog.read().await;

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
                    8 => ReplicaRole::Down,
                    9 => ReplicaRole::Rollback,
                    10 => ReplicaRole::Removed,
                    other => {
                        tracing::warn!(
                            member = %m.name,
                            state = other,
                            state_str = %m.state_str,
                            "unrecognized RS member state"
                        );
                        ReplicaRole::Unknown
                    }
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

                // Resolve rs.status() member name to stone_name via catalog
                let stone_name = member_catalog
                    .resolve_name(&m.name)
                    .map(|s| s.to_string())
                    .or_else(|| {
                        // Fallback: direct match on mongo_endpoint field
                        instances
                            .iter()
                            .find(|i| i.mongo_endpoint == m.name)
                            .map(|i| i.stone_name.clone())
                    })
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

        drop(member_catalog);

        // ── Quorum loss recovery ──────────────────────────────────
        // No PRIMARY + at least one healthy SECONDARY + at least one DOWN member
        // → force-reconfig to healthy members only, restoring write availability.
        {
            let probe_rs = ReplicaSetState {
                rs_name: rs_name.clone(),
                initialized: true,
                members: members.clone(),
                connection_string: None,
                last_updated: chrono::Utc::now(),
                cache: None,
                oplog: None,
            };

            if let Some(ql) = membership::detect_quorum_loss(&probe_rs) {
                tracing::warn!(
                    fqn = %fqn,
                    healthy = ?ql.healthy_endpoints,
                    evicted = ?ql.evicted_endpoints,
                    "quorum loss detected — force-reconfiguring to healthy members"
                );

                let desired: Vec<&str> =
                    ql.healthy_endpoints.iter().map(|s| s.as_str()).collect();
                match client.rs_reconfig_members(&rs_name, &desired).await {
                    Ok(()) => {
                        tracing::info!(
                            fqn = %fqn,
                            members = ?desired,
                            "quorum recovered — RS reconfigured to healthy members"
                        );
                        state
                            .emit_event(
                                "rs.quorum.recovered",
                                &serde_json::json!({
                                    "fqn": fqn,
                                    "healthy_members": ql.healthy_endpoints,
                                    "evicted_members": ql.evicted_endpoints,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = ?e,
                            fqn = %fqn,
                            "quorum recovery reconfig failed — will retry next cycle"
                        );
                    }
                }

                // Update instance registry — roles always, health only when
                // we have better info than Phase 1's Offline/Down classification.
                for member in &members {
                    update_instance_role(state, &member.endpoint, member.role.clone()).await;
                    if member.healthy {
                        update_instance_health(state, &member.endpoint, InstanceHealth::Healthy)
                            .await;
                    } else {
                        match member.role {
                            ReplicaRole::Recovering | ReplicaRole::Rollback | ReplicaRole::Startup => {
                                update_instance_health(
                                    state,
                                    &member.endpoint,
                                    InstanceHealth::Degraded,
                                )
                                .await;
                            }
                            // Down/Removed/Unknown: Phase 1 already set Offline or Down
                            // with the Moss check — don't overwrite with less info.
                            _ => {}
                        }
                    }
                }

                // Save the current RS state so detect_member_changes has an
                // accurate baseline next cycle (prevents stale role flapping).
                state.update_replica_set(fqn, probe_rs).await;
                // Next health cycle will pick up the post-reconfig state
                return None;
            }
        }

        // Update instance roles and health in state.
        // rs.status() reports health for ALL members, not just the one we connected to.
        // For unhealthy members, Phase 1 already classified them as Offline or Down
        // via the Moss check — only override for Degraded (Recovering/Rollback/Startup).
        for member in &members {
            update_instance_role(state, &member.endpoint, member.role.clone()).await;

            if member.healthy {
                update_instance_health(state, &member.endpoint, InstanceHealth::Healthy).await;
            } else {
                match member.role {
                    ReplicaRole::Recovering | ReplicaRole::Rollback | ReplicaRole::Startup => {
                        update_instance_health(state, &member.endpoint, InstanceHealth::Degraded)
                            .await;
                    }
                    // Phase 1 set Offline or Down — don't overwrite.
                    _ => {}
                }
            }
        }

        let conn_string = build_connection_string(&members, &status.set_name);

        // ── Oplog health (best-effort, only from primary) ──
        let max_lag = members
            .iter()
            .filter_map(|m| m.lag_seconds)
            .fold(0.0_f64, f64::max);

        let oplog_snapshot = if let Ok(repl_info) = client.replication_info().await {
            Some(oplog::evaluate_oplog(
                repl_info.oplog_window_secs,
                repl_info.oplog_used_mb,
                repl_info.oplog_size_mb,
                max_lag,
            ))
        } else {
            None
        };

        // ── WiredTiger cache (best-effort) ──
        let cache_snapshot = if let Ok(server_status) = client.server_status().await {
            cache_advisor::parse_cache_status(&server_status).map(|status| {
                let recs = cache_advisor::evaluate_cache(&status, 0, 0);
                let health = recs
                    .iter()
                    .map(|r| &r.severity)
                    .max_by_key(|s| match s {
                        cache_advisor::CacheHealth::Healthy => 0,
                        cache_advisor::CacheHealth::Warning => 1,
                        cache_advisor::CacheHealth::Critical => 2,
                    })
                    .cloned()
                    .unwrap_or(cache_advisor::CacheHealth::Healthy);
                CacheSnapshot {
                    status,
                    health,
                    recommendations: recs,
                }
            })
        } else {
            None
        };

        return Some(ReplicaSetState {
            rs_name: status.set_name,
            initialized: true,
            members,
            connection_string: Some(conn_string),
            last_updated: chrono::Utc::now(),
            cache: cache_snapshot,
            oplog: oplog_snapshot,
        });
    }

    None
}

/// Update a single instance's health in shared state.
///
/// Resolves `mongo_endpoint` through the catalog to find the stone_name key.
async fn update_instance_health(state: &AppState, mongo_endpoint: &str, health: InstanceHealth) {
    let stone_name = state.resolve_endpoint(mongo_endpoint).await;
    let mut reg = state.instances.write().await;
    let key = stone_name.as_deref().unwrap_or(mongo_endpoint);
    if let Some(inst) = reg.get_mut(key) {
        if inst.health != health {
            tracing::info!(
                stone = %key,
                from = ?inst.health,
                to = ?health,
                "instance health changed"
            );
        }
        inst.health = health;
        inst.last_seen = Instant::now();
    }
}

/// Update a single instance's role in shared state.
///
/// Resolves `mongo_endpoint` through the catalog to find the stone_name key.
async fn update_instance_role(state: &AppState, mongo_endpoint: &str, role: ReplicaRole) {
    let stone_name = state.resolve_endpoint(mongo_endpoint).await;
    let mut reg = state.instances.write().await;
    let key = stone_name.as_deref().unwrap_or(mongo_endpoint);
    if let Some(inst) = reg.get_mut(key) {
        if inst.role.as_ref() != Some(&role) {
            tracing::info!(
                stone = %key,
                from = ?inst.role,
                to = %role,
                "instance role changed"
            );
            inst.role = Some(role);
        }
    }
}

/// Clear stale endpoints on an offline instance.
///
/// DHCP will assign a new IP when the stone returns; keeping the old IP
/// is misleading.  The tools stream will set fresh endpoints when it
/// re-discovers the stone (`OfferingDiscovered`, `ready=true`).
async fn flush_stale_endpoints(state: &AppState, mongo_endpoint: &str) {
    let stone_name = state.resolve_endpoint(mongo_endpoint).await;
    let mut reg = state.instances.write().await;
    let key = stone_name.as_deref().unwrap_or(mongo_endpoint);
    if let Some(inst) = reg.get_mut(key) {
        inst.mongo_endpoint.clear();
        inst.moss_endpoint.clear();
    }
}

/// Classify an unreachable MongoDB instance as `Offline` or `Down`.
///
/// Probes the stone's Moss API (`/health`). If Moss responds, the stone
/// is online but MongoDB specifically is not responding → `Down`.
/// If Moss is also unreachable, the stone is offline → `Offline`.
async fn classify_unreachable(moss_endpoint: &str) -> InstanceHealth {
    let url = format!("{}/health", moss_endpoint.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();
    match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => InstanceHealth::Down,
        _ => InstanceHealth::Offline,
    }
}
