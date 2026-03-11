//! Replica manager — single authority for replica set lifecycle and health.
//!
//! Merges the responsibilities of the former `bootstrap` and `health_monitor`
//! tasks into a unified executor with two entry points:
//!
//! - `check()` — read-only health assessment (probe, observe, report)
//! - `reconcile()` — full lifecycle (config patches → probe → classify → act
//!   → membership → removals → observe)
//!
//! The `Conductor` (in `tasks/conductor.rs`) drives this module:
//! - On reactive signal → `reconcile()` for affected FQNs
//! - On periodic timer  → `check()`, then `reconcile()` if broken

use crate::app_state::AppState;
use crate::domain::cache_advisor;
use crate::domain::group_state::{
    classify_group, compute_drift_mapping, GroupAction, GroupPhase, GroupState, InstanceProbe,
    KnownMember,
};
use crate::domain::membership;
use crate::domain::oplog;
use crate::domain::types::*;
use crate::infra::mongo_client::MongoClient;
use garden_common::offerings::OfferingFqn;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The owner name for config patches applied by this orchestrator.
const CONFIG_PATCH_OWNER: &str = "mongodb-orchestrator";

/// Minimum interval between repeated log lines for the same severity.
const LOG_COOLDOWN: Duration = Duration::from_secs(300);

/// Result of a `check()` call — tells the conductor whether reconciliation is needed.
pub struct CheckResult {
    pub needs_reconciliation: bool,
}

/// Snapshot from a unified probe sweep across all active instances.
struct ProbeSnapshot {
    /// Per-instance classification for the group classifier.
    probes: Vec<(String, InstanceProbe)>,
    /// Reachable instances with their live MongoClient connections.
    reachable: Vec<(MongoInstance, MongoClient)>,
    /// The active instances that were probed (Unknown | Healthy | Degraded).
    active_instances: Vec<MongoInstance>,
}

/// Single authority for all replica set operations.
pub struct ReplicaManager {
    state: AppState,
    http_client: reqwest::Client,
    cache_log_state: Mutex<HashMap<OfferingFqn, (cache_advisor::CacheHealth, Instant)>>,
    oplog_log_state: Mutex<HashMap<OfferingFqn, (oplog::OplogSeverity, Instant)>>,
}

impl ReplicaManager {
    pub fn new(state: AppState) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            state,
            http_client,
            cache_log_state: Mutex::new(HashMap::new()),
            oplog_log_state: Mutex::new(HashMap::new()),
        }
    }

    // ========================================================================
    // check() — read-only health assessment
    // ========================================================================

    /// Probe all instances for a FQN, update health/roles/metrics, and report
    /// whether reconciliation is needed.
    pub async fn check(
        &self,
        fqn: &OfferingFqn,
        instances: &[MongoInstance],
    ) -> CheckResult {
        if instances.is_empty() {
            return CheckResult { needs_reconciliation: false };
        }

        let snap = self.probe_sweep(fqn, instances).await;

        if snap.reachable.is_empty() {
            // All unreachable — can't assess, but we can't reconcile either
            return CheckResult { needs_reconciliation: false };
        }

        // Observe health/roles/metrics (read-only state updates)
        let new_rs_state = self.observe(fqn, instances, &snap).await;

        // Determine if reconciliation is needed
        let needs_reconciliation = self.assess_needs_reconciliation(fqn, &snap, &new_rs_state).await;

        CheckResult { needs_reconciliation }
    }

    // ========================================================================
    // reconcile() — full lifecycle
    // ========================================================================

    /// Full lifecycle management for a FQN group:
    /// 1. Config patches (ensure `--replSet` is applied)
    /// 2. Probe sweep
    /// 3. Classify group state
    /// 4. Execute lifecycle action (initiate / reconfig-drift / wait)
    /// 5. Reconcile membership (add/remove members)
    /// 6. Quorum recovery
    /// 7. Execute pending removals
    /// 8. Observe health/metrics
    pub async fn reconcile(
        &self,
        fqn: &OfferingFqn,
        instances: &[MongoInstance],
    ) {
        if instances.is_empty() {
            return;
        }

        let rs_name = derive_replica_set_name(fqn);

        // Filter to active instances
        let active_instances: Vec<MongoInstance> = instances
            .iter()
            .filter(|i| !matches!(i.health, InstanceHealth::Offline))
            .cloned()
            .collect();

        // Step 1: Ensure config patches are applied BEFORE any RS operations.
        // This eliminates the race where a member gets added before --replSet
        // is configured.
        if !active_instances.is_empty() {
            self.ensure_config_patches(&active_instances, &rs_name).await;
        }

        // Step 2: Probe sweep
        let snap = self.probe_sweep(fqn, instances).await;

        tracing::debug!(
            fqn = %fqn,
            probes = ?snap.probes.iter().map(|(ep, p)| format!("{}={:?}", ep, p)).collect::<Vec<_>>(),
            "reconcile probe results"
        );

        // Step 3: Classify — pure domain logic decides the action
        let action = classify_group(&snap.probes, &rs_name);
        let group_state = self.state.group_for(fqn).await;

        // Step 4: Execute pending removals (if we have RS state)
        let rs_state = self.state.replica_set_for(fqn).await;
        if let Some(ref rs) = rs_state {
            self.execute_pending_removals(rs, fqn).await;
        }

        // Step 5: Execute the classified action
        match action {
            GroupAction::Healthy => {
                // RS is operational — proceed to membership reconciliation.
                if group_state.as_ref().map(|g| &g.phase) != Some(&GroupPhase::Healthy) {
                    tracing::info!(fqn = %fqn, rs_name = %rs_name, "RS operational");
                    if let Some(mut gs) = group_state.clone() {
                        gs.phase = GroupPhase::Healthy;
                        gs.last_updated = chrono::Utc::now();
                        self.state.update_group(fqn, gs).await;
                    }
                }

                // Step 6: Reconcile membership (only when Healthy)
                self.reconcile_membership(fqn, &rs_name, &snap).await;
            }

            GroupAction::WaitForConfig => {
                tracing::info!(
                    fqn = %fqn,
                    rs_name = %rs_name,
                    "waiting — config file patch pending on one or more instances"
                );
                let gs = group_state.unwrap_or_else(|| GroupState {
                    rs_name: rs_name.clone(),
                    phase: GroupPhase::Configuring,
                    known_members: vec![],
                    last_updated: chrono::Utc::now(),
                });
                self.state
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

                match self.initiate_replica_set(&endpoint, &rs_name).await {
                    Ok(new_state) => {
                        tracing::info!(rs_name = %rs_name, "replica set initiated successfully");
                        let known = new_state
                            .members
                            .iter()
                            .map(|m| KnownMember {
                                stone_name: m.stone_name.clone(),
                                endpoint: m.endpoint.clone(),
                                member_id: -1,
                            })
                            .collect();
                        self.state
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
                        self.state.update_replica_set(fqn, new_state).await;
                        self.state
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
                return; // Let the newly initiated RS stabilize
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

                match self
                    .execute_reconfig_drift(
                        &connect_to,
                        &rs_name,
                        &desired,
                        &snap.active_instances,
                        group_state.as_ref(),
                    )
                    .await
                {
                    Ok(()) => {
                        let known: Vec<KnownMember> = desired
                            .iter()
                            .map(|ep| {
                                let stone_name = snap.active_instances
                                    .iter()
                                    .find(|i| i.mongo_endpoint == *ep)
                                    .map(|i| i.stone_name.clone())
                                    .unwrap_or_else(|| ep.clone());
                                KnownMember {
                                    stone_name,
                                    endpoint: ep.clone(),
                                    member_id: -1,
                                }
                            })
                            .collect();

                        self.state
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

                        self.state
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
                        self.state
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

        // Step 7: Observe health/metrics (also done in check, but reconcile
        // may have changed things — re-observe for accurate state)
        self.observe(fqn, instances, &snap).await;
    }

    // ========================================================================
    // Unified probe sweep
    // ========================================================================

    /// Probe all active instances in one pass. Returns:
    /// - `probes`: per-instance classification for the group classifier
    /// - `reachable`: instances with live MongoClient connections
    /// - `active_instances`: the filtered set that was probed
    async fn probe_sweep(
        &self,
        _fqn: &OfferingFqn,
        instances: &[MongoInstance],
    ) -> ProbeSnapshot {
        let active_instances: Vec<MongoInstance> = instances
            .iter()
            .filter(|i| !matches!(i.health, InstanceHealth::Offline))
            .cloned()
            .collect();

        let mut probes = Vec::with_capacity(active_instances.len());
        let mut reachable: Vec<(MongoInstance, MongoClient)> = Vec::new();

        for instance in &active_instances {
            let client = match MongoClient::connect(&instance.mongo_endpoint).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(
                        endpoint = %instance.mongo_endpoint,
                        error = %e,
                        "could not connect to MongoDB instance"
                    );
                    // Classify unreachable: Offline (stone down) vs Down (mongo down)
                    let health = classify_unreachable(&self.http_client, &instance.moss_endpoint).await;
                    self.update_instance_health(&instance.mongo_endpoint, health.clone()).await;
                    if health == InstanceHealth::Offline {
                        self.flush_stale_endpoints(&instance.mongo_endpoint).await;
                    }
                    probes.push((instance.mongo_endpoint.clone(), InstanceProbe::Unreachable));
                    continue;
                }
            };

            if !client.ping().await {
                let health = classify_unreachable(&self.http_client, &instance.moss_endpoint).await;
                self.update_instance_health(&instance.mongo_endpoint, health.clone()).await;
                if health == InstanceHealth::Offline {
                    self.flush_stale_endpoints(&instance.mongo_endpoint).await;
                }
                probes.push((instance.mongo_endpoint.clone(), InstanceProbe::Unreachable));
                continue;
            }

            // Collect version info for compatibility checks
            if let Ok(info) = client.build_info().await {
                self.update_instance_version(
                    &instance.mongo_endpoint,
                    &info.version,
                    info.min_wire_version,
                    info.max_wire_version,
                )
                .await;
            }

            // Connected and responsive — classify via rs.status()
            match client.rs_status().await {
                Ok(_status) => {
                    self.update_instance_health(&instance.mongo_endpoint, InstanceHealth::Healthy).await;
                    probes.push((instance.mongo_endpoint.clone(), InstanceProbe::Active));
                    reachable.push((instance.clone(), client));
                }
                Err(e) => {
                    let err_str = format!("{:#}", e);

                    if err_str.contains("NotYetInitialized")
                        || err_str.contains("no replset config")
                    {
                        self.update_instance_health(&instance.mongo_endpoint, InstanceHealth::Healthy).await;
                        probes.push((
                            instance.mongo_endpoint.clone(),
                            InstanceProbe::NotInitialized,
                        ));
                        reachable.push((instance.clone(), client));
                    } else if err_str.contains("NoReplicationEnabled")
                        || err_str.contains("error 76")
                    {
                        tracing::debug!(
                            endpoint = %instance.mongo_endpoint,
                            "instance not configured for replication (config file patch pending)"
                        );
                        self.update_instance_health(&instance.mongo_endpoint, InstanceHealth::Healthy).await;
                        probes.push((
                            instance.mongo_endpoint.clone(),
                            InstanceProbe::ConfigPending,
                        ));
                        // Keep client — needed for post-restart reconfig
                        reachable.push((instance.clone(), client));
                    } else if err_str.contains("InvalidReplicaSetConfig")
                        || err_str.contains("error 93")
                    {
                        tracing::debug!(
                            endpoint = %instance.mongo_endpoint,
                            "instance has stale RS config (IP drift)"
                        );
                        self.update_instance_health(&instance.mongo_endpoint, InstanceHealth::Degraded).await;
                        probes.push((
                            instance.mongo_endpoint.clone(),
                            InstanceProbe::StaleConfig,
                        ));
                        reachable.push((instance.clone(), client));
                    } else {
                        tracing::debug!(
                            endpoint = %instance.mongo_endpoint,
                            error = %e,
                            "rs.status() returned unrecognized error"
                        );
                        probes.push((
                            instance.mongo_endpoint.clone(),
                            InstanceProbe::Unreachable,
                        ));
                    }
                }
            }
        }

        // Post-probe: detect version incompatibility among reachable instances.
        // Find the primary's version, then mark incompatible instances.
        let primary_version = self.find_primary_version(&reachable).await;
        if let Some(ref pv) = primary_version {
            for (inst, _) in &reachable {
                if let Some(ref cv) = inst.server_version {
                    if !major_versions_compatible(pv, cv) {
                        tracing::info!(
                            stone = %inst.stone_name,
                            primary_version = %pv,
                            instance_version = %cv,
                            "instance version incompatible with RS primary — marking incompatible"
                        );
                        self.update_instance_health(
                            &inst.mongo_endpoint,
                            InstanceHealth::Incompatible,
                        )
                        .await;
                    }
                }
            }
        }

        ProbeSnapshot {
            probes,
            reachable,
            active_instances,
        }
    }

    /// Find the primary's server version from reachable instances.
    async fn find_primary_version(
        &self,
        reachable: &[(MongoInstance, MongoClient)],
    ) -> Option<String> {
        for (inst, client) in reachable {
            if let Ok(status) = client.rs_status().await {
                // Find the primary endpoint from rs.status()
                if let Some(primary_member) = status.members.iter().find(|m| m.state == 1) {
                    // If this instance IS the primary, return its version
                    if primary_member.is_self {
                        return inst.server_version.clone();
                    }
                    // Otherwise find the primary in our reachable set
                    return reachable
                        .iter()
                        .find(|(i, _)| i.mongo_endpoint == primary_member.name)
                        .and_then(|(i, _)| i.server_version.clone());
                }
            }
        }
        None
    }

    // ========================================================================
    // observe() — health/roles/metrics/events
    // ========================================================================

    /// Update instance health, roles, replication lag, oplog, cache metrics,
    /// and emit events. Returns the new RS state if one could be built.
    async fn observe(
        &self,
        fqn: &OfferingFqn,
        instances: &[MongoInstance],
        snap: &ProbeSnapshot,
    ) -> Option<ReplicaSetState> {
        let rs_name = derive_replica_set_name(fqn);

        if snap.reachable.is_empty() {
            return None;
        }

        let old_rs_state = self.state.replica_set_for(fqn).await;

        // Try to get rs.status() from a reachable Active instance
        for (instance, client) in &snap.reachable {
            let status = match client.rs_status().await {
                Ok(s) => s,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("NotYetInitialized") || err_str.contains("no replset config") {
                        // RS not yet initialized — report that
                        let uninit_state = ReplicaSetState {
                            rs_name,
                            initialized: false,
                            members: vec![],
                            connection_string: None,
                            last_updated: chrono::Utc::now(),
                            cache: None,
                            oplog: None,
                        };
                        self.state.update_replica_set(fqn, uninit_state.clone()).await;
                        return Some(uninit_state);
                    }
                    tracing::debug!(error = %e, endpoint = %instance.mongo_endpoint, "rs.status() failed in observe");
                    continue;
                }
            };

            let primary_optime = status
                .members
                .iter()
                .find(|m| m.state == 1)
                .and_then(|m| m.optime_ts);

            let member_catalog = self.state.catalog.read().await;

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

                    let stone_name = member_catalog
                        .resolve_name(&m.name)
                        .map(|s| s.to_string())
                        .or_else(|| {
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

            // ── Quorum loss recovery ──
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
                            self.state
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

                    // Update roles/health from the pre-reconfig state
                    for member in &members {
                        self.update_instance_role(&member.endpoint, member.role.clone()).await;
                        if member.healthy {
                            self.update_instance_health(&member.endpoint, InstanceHealth::Healthy).await;
                        } else {
                            match member.role {
                                ReplicaRole::Recovering | ReplicaRole::Rollback | ReplicaRole::Startup => {
                                    self.update_instance_health(&member.endpoint, InstanceHealth::Degraded).await;
                                }
                                _ => {}
                            }
                        }
                    }

                    self.state.update_replica_set(fqn, probe_rs).await;
                    return None; // Next cycle picks up post-reconfig state
                }
            }

            // Update instance roles and health
            for member in &members {
                self.update_instance_role(&member.endpoint, member.role.clone()).await;
                if member.healthy {
                    self.update_instance_health(&member.endpoint, InstanceHealth::Healthy).await;
                } else {
                    match member.role {
                        ReplicaRole::Recovering | ReplicaRole::Rollback | ReplicaRole::Startup => {
                            self.update_instance_health(&member.endpoint, InstanceHealth::Degraded).await;
                        }
                        _ => {}
                    }
                }
            }

            // Detect membership changes
            if let Some(ref old) = old_rs_state {
                let changes = membership::detect_member_changes(old, &ReplicaSetState {
                    rs_name: rs_name.clone(),
                    initialized: true,
                    members: members.clone(),
                    connection_string: None,
                    last_updated: chrono::Utc::now(),
                    cache: None,
                    oplog: None,
                });
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

                if let Some(pc) = membership::detect_primary_change(old, &ReplicaSetState {
                    rs_name: rs_name.clone(),
                    initialized: true,
                    members: members.clone(),
                    connection_string: None,
                    last_updated: chrono::Utc::now(),
                    cache: None,
                    oplog: None,
                }) {
                    tracing::warn!(
                        old = ?pc.old_primary,
                        new = ?pc.new_primary,
                        "PRIMARY CHANGE detected"
                    );
                    self.state
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
                    self.state
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

            let new_rs_state = ReplicaSetState {
                rs_name: status.set_name,
                initialized: true,
                members,
                connection_string: Some(conn_string),
                last_updated: chrono::Utc::now(),
                cache: cache_snapshot,
                oplog: oplog_snapshot,
            };

            // Rate-limited cache logging
            if let Some(ref cache) = new_rs_state.cache {
                let should_log = {
                    let log_state = self.cache_log_state.lock().unwrap();
                    match log_state.get(fqn) {
                        Some((prev_health, last_logged)) => {
                            *prev_health != cache.health
                                || last_logged.elapsed() >= LOG_COOLDOWN
                        }
                        None => cache.health != cache_advisor::CacheHealth::Healthy,
                    }
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
                self.cache_log_state
                    .lock()
                    .unwrap()
                    .insert(fqn.clone(), (cache.health.clone(), Instant::now()));

                self.state
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

            // Rate-limited oplog logging
            if let Some(ref oplog_health) = new_rs_state.oplog {
                let should_log = {
                    let log_state = self.oplog_log_state.lock().unwrap();
                    match log_state.get(fqn) {
                        Some((prev_severity, last_logged)) => {
                            *prev_severity != oplog_health.severity
                                || last_logged.elapsed() >= LOG_COOLDOWN
                        }
                        None => oplog_health.severity != oplog::OplogSeverity::Healthy,
                    }
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
                self.oplog_log_state
                    .lock()
                    .unwrap()
                    .insert(fqn.clone(), (oplog_health.severity.clone(), Instant::now()));

                if oplog_health.severity != oplog::OplogSeverity::Healthy {
                    self.state
                        .emit_event(
                            "oplog.health",
                            &serde_json::to_string(&oplog_health).unwrap_or_default(),
                        )
                        .await;
                }
            }

            // Persist
            self.state.update_group_members(fqn, &new_rs_state).await;
            self.state.update_replica_set(fqn, new_rs_state.clone()).await;

            return Some(new_rs_state);
        }

        None
    }

    // ========================================================================
    // assess_needs_reconciliation — determine if reconcile() is needed
    // ========================================================================

    async fn assess_needs_reconciliation(
        &self,
        fqn: &OfferingFqn,
        snap: &ProbeSnapshot,
        new_rs_state: &Option<ReplicaSetState>,
    ) -> bool {
        // Check 1: lifecycle action needed (not Healthy)
        let rs_name = derive_replica_set_name(fqn);
        let action = classify_group(&snap.probes, &rs_name);
        if !matches!(action, GroupAction::Healthy) {
            return true;
        }

        // Check 2: pending removals
        let actions = self.state.pending_actions_snapshot().await;
        let has_pending = actions.iter().any(|a| match a {
            PendingAction::RemoveMember { fqn: action_fqn, .. } => action_fqn == fqn,
        });
        if has_pending {
            return true;
        }

        // Check 3: membership mismatch (reachable set != RS members)
        if let Some(rs) = new_rs_state {
            if rs.initialized {
                let rs_hosts: HashSet<&str> = rs.members.iter().map(|m| m.endpoint.as_str()).collect();
                let registry_hosts: HashSet<&str> = snap.reachable
                    .iter()
                    .map(|(i, _)| i.mongo_endpoint.as_str())
                    .collect();
                if rs_hosts != registry_hosts {
                    return true;
                }
            }
        }

        false
    }

    // ========================================================================
    // Membership reconciliation (from health_monitor Phase 2)
    // ========================================================================

    /// Compare RS membership against the reachable set and reconcile.
    async fn reconcile_membership(
        &self,
        fqn: &OfferingFqn,
        rs_name: &str,
        snap: &ProbeSnapshot,
    ) {
        if snap.reachable.is_empty() {
            return;
        }

        let registry_hosts: HashSet<&str> = snap.reachable
            .iter()
            .map(|(i, _)| i.mongo_endpoint.as_str())
            .collect();

        let registry_name_to_endpoint: HashMap<&str, &str> = snap.reachable
            .iter()
            .map(|(i, _)| (i.stone_name.as_str(), i.mongo_endpoint.as_str()))
            .collect();

        let registry_stone_names: HashSet<&str> =
            registry_name_to_endpoint.keys().copied().collect();

        for (instance, client) in &snap.reachable {
            let status = match client.rs_status().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::debug!(error = %e, endpoint = %instance.mongo_endpoint, "rs.status() failed in membership reconciliation");
                    continue;
                }
            };

            let rs_hosts: HashSet<&str> = status.members.iter().map(|m| m.name.as_str()).collect();

            // Build stone_name→RS_endpoint map from RS members
            let rs_name_to_endpoint: HashMap<String, String> = {
                let catalog = self.state.catalog.read().await;
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

            let rs_resolved_names: HashSet<&str> =
                rs_name_to_endpoint.keys().map(|s| s.as_str()).collect();

            let all_rs_resolved = rs_name_to_endpoint.len() == status.members.len();
            let names_match = all_rs_resolved && rs_resolved_names == registry_stone_names;
            let endpoints_match = rs_hosts == registry_hosts;

            if names_match && endpoints_match {
                // Perfect match — nothing to do
                return;
            } else if names_match && !endpoints_match {
                // IP drift: same stones but endpoints changed (DHCP renewal)
                let mut drift_details = Vec::new();
                let mut old_to_new: HashMap<String, String> = HashMap::new();
                for (stone, old_ep) in &rs_name_to_endpoint {
                    if let Some(new_ep) = registry_name_to_endpoint.get(stone.as_str()) {
                        if old_ep != *new_ep {
                            drift_details.push(format!("{}: {} → {}", stone, old_ep, new_ep));
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
                    .rs_reconfig_members_with_mapping(rs_name, &desired, &old_to_new)
                    .await
                {
                    tracing::warn!(error = ?e, "RS reconfig (IP drift) failed — will retry next cycle");
                } else {
                    tracing::info!(
                        fqn = %fqn,
                        members = ?desired,
                        "RS config updated for IP drift — member _ids preserved"
                    );
                    self.state
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
                return;
            } else if !names_match {
                // Membership change: different stones (added/removed)
                //
                // Before adding new members, verify they are RS-ready (have
                // `--replSet` configured). This is the safety net for the edge
                // case where the config patch was just applied but the container
                // hasn't restarted yet.
                let new_endpoints: Vec<&str> = registry_hosts
                    .iter()
                    .filter(|ep| !rs_hosts.contains(*ep))
                    .copied()
                    .collect();

                let mut defer = false;
                for ep in &new_endpoints {
                    if let Some((_, new_client)) = snap.reachable.iter().find(|(i, _)| i.mongo_endpoint.as_str() == *ep) {
                        if let Err(e) = new_client.rs_status().await {
                            let err_str = format!("{:#}", e);
                            if err_str.contains("NoReplicationEnabled") || err_str.contains("error 76") {
                                tracing::info!(
                                    endpoint = %ep,
                                    "new member not yet configured for replication — deferring RS add"
                                );
                                defer = true;
                            }
                        }
                    }
                }

                if defer {
                    return;
                }

                // Check wire version / major version compatibility for new members.
                // Get the RS primary's server version as the reference.
                let primary_version: Option<String> = {
                    let rs_status = client.rs_status().await.ok();
                    let primary_ep = rs_status.as_ref().and_then(|s| {
                        s.members.iter().find(|m| m.state == 1).map(|m| m.name.as_str())
                    });
                    if let Some(pep) = primary_ep {
                        // Look up version from our reachable set
                        snap.reachable
                            .iter()
                            .find(|(i, _)| i.mongo_endpoint.as_str() == pep)
                            .and_then(|(i, _)| i.server_version.clone())
                    } else {
                        None
                    }
                };

                let mut incompatible_eps: Vec<&str> = Vec::new();
                if let Some(ref rs_ver) = primary_version {
                    for ep in &new_endpoints {
                        if let Some((inst, _)) = snap.reachable.iter().find(|(i, _)| i.mongo_endpoint.as_str() == *ep) {
                            if let Some(ref cand_ver) = inst.server_version {
                                if !major_versions_compatible(rs_ver, cand_ver) {
                                    tracing::warn!(
                                        endpoint = %ep,
                                        rs_version = %rs_ver,
                                        candidate_version = %cand_ver,
                                        "incompatible MongoDB version — cannot add to replica set"
                                    );
                                    self.update_instance_health(ep, InstanceHealth::Incompatible).await;
                                    incompatible_eps.push(ep);
                                }
                            }
                        }
                    }
                }

                // Filter out incompatible endpoints from the desired set
                let compatible_hosts: HashSet<&str> = registry_hosts
                    .iter()
                    .filter(|h| !incompatible_eps.contains(h))
                    .copied()
                    .collect();

                // If all new endpoints were incompatible, nothing to reconfig
                if compatible_hosts == rs_hosts {
                    return;
                }

                tracing::warn!(
                    fqn = %fqn,
                    rs_members = ?rs_hosts,
                    logical_set = ?compatible_hosts,
                    incompatible = ?incompatible_eps,
                    "RS members don't match logical set — rebuilding RS config"
                );

                let desired: Vec<&str> = compatible_hosts.iter().copied().collect();
                if let Err(e) = client.rs_reconfig_members(rs_name, &desired).await {
                    tracing::warn!(error = ?e, "RS reconfig failed — will retry next cycle");
                } else {
                    tracing::info!(fqn = %fqn, members = ?desired, "RS config rebuilt to match logical set");
                    self.state
                        .emit_event(
                            "rs.reconfig",
                            &serde_json::json!({ "fqn": fqn, "members": desired }).to_string(),
                        )
                        .await;
                }
                return;
            }

            // If we got here, match was perfect — no action needed
            return;
        }
    }

    // ========================================================================
    // Config patch management (from bootstrap)
    // ========================================================================

    /// Ensure each instance has the replica set config file patch via Moss API.
    async fn ensure_config_patches(&self, instances: &[MongoInstance], rs_name: &str) {
        for instance in instances {
            if let Err(e) = self.apply_repl_set_patch(instance, rs_name).await {
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
        &self,
        instance: &MongoInstance,
        rs_name: &str,
    ) -> anyhow::Result<()> {
        let service_name = instance.fqn.fqn();
        let base_url = instance.moss_endpoint.trim_end_matches('/');
        let encoded_name = urlencoding::encode(&service_name);
        let config_url = format!(
            "{}/api/v1/stone/services/{}/config",
            base_url, encoded_name
        );

        // Check if patch already exists
        let check_url = format!("{}?owner={}", config_url, CONFIG_PATCH_OWNER);
        let resp = self.http_client.get(&check_url).send().await?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await?;
            let patches = body
                .get("patches")
                .and_then(|p| p.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            if patches > 0 {
                return Ok(());
            }
        }

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

        let resp = self.http_client
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

    // ========================================================================
    // Replica set operations (from bootstrap)
    // ========================================================================

    /// Execute a force-reconfig to fix IP drift.
    async fn execute_reconfig_drift(
        &self,
        connect_to: &str,
        rs_name: &str,
        desired: &[String],
        active_instances: &[MongoInstance],
        group_state: Option<&GroupState>,
    ) -> anyhow::Result<()> {
        let client = MongoClient::connect(connect_to).await?;

        let (_config_rs_name, config_members) = client.rs_config().await?;

        let old_to_new = if let Some(gs) = group_state {
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
        &self,
        endpoint: &str,
        rs_name: &str,
    ) -> anyhow::Result<ReplicaSetState> {
        let client = MongoClient::connect(endpoint).await?;
        let freshly_initiated = client.rs_initiate(rs_name).await?;

        if freshly_initiated {
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

    /// Execute pending removal actions for a specific FQN.
    async fn execute_pending_removals(
        &self,
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
            None => return,
        };

        let actions = self.state.pending_actions_snapshot().await;
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
                        tracing::info!(member = %mongo_endpoint, "member removed from replica set");
                        self.state
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
                        continue;
                    }
                }
            }

            self.state.complete_action(mongo_endpoint).await;
            if let Some(stone_name) = self.state.resolve_endpoint(mongo_endpoint).await {
                self.state.remove_instance(&stone_name).await;
            }
        }
    }

    // ========================================================================
    // Instance state helpers
    // ========================================================================

    /// Update a single instance's health in shared state.
    async fn update_instance_health(&self, mongo_endpoint: &str, health: InstanceHealth) {
        let stone_name = self.state.resolve_endpoint(mongo_endpoint).await;
        let mut reg = self.state.instances.write().await;
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
    async fn update_instance_role(&self, mongo_endpoint: &str, role: ReplicaRole) {
        let stone_name = self.state.resolve_endpoint(mongo_endpoint).await;
        let mut reg = self.state.instances.write().await;
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

    /// Update a single instance's version info in shared state.
    async fn update_instance_version(
        &self,
        mongo_endpoint: &str,
        version: &str,
        min_wire: i32,
        max_wire: i32,
    ) {
        let stone_name = self.state.resolve_endpoint(mongo_endpoint).await;
        let mut reg = self.state.instances.write().await;
        let key = stone_name.as_deref().unwrap_or(mongo_endpoint);
        if let Some(inst) = reg.get_mut(key) {
            inst.server_version = Some(version.to_string());
            inst.wire_version_range = Some((min_wire, max_wire));
        }
    }

    /// Clear stale endpoints on an offline instance.
    async fn flush_stale_endpoints(&self, mongo_endpoint: &str) {
        let stone_name = self.state.resolve_endpoint(mongo_endpoint).await;
        let mut reg = self.state.instances.write().await;
        let key = stone_name.as_deref().unwrap_or(mongo_endpoint);
        if let Some(inst) = reg.get_mut(key) {
            inst.mongo_endpoint.clear();
            inst.moss_endpoint.clear();
        }
    }
}

// ============================================================================
// Free functions (not tied to ReplicaManager state)
// ============================================================================

/// Remove a member from the replica set via the primary.
async fn remove_member(primary_endpoint: &str, member_endpoint: &str) -> anyhow::Result<()> {
    let client = MongoClient::connect(primary_endpoint).await?;
    client.rs_remove(member_endpoint).await
}

/// Classify an unreachable MongoDB instance as `Offline` or `Down`.
///
/// Probes the stone's Moss API (`/health`). If Moss responds, the stone
/// is online but MongoDB specifically is not responding → `Down`.
/// If Moss is also unreachable, the stone is offline → `Offline`.
async fn classify_unreachable(http_client: &reqwest::Client, moss_endpoint: &str) -> InstanceHealth {
    let url = format!("{}/health", moss_endpoint.trim_end_matches('/'));
    match http_client.get(&url).send().await {
        Ok(r) if r.status().is_success() => InstanceHealth::Down,
        _ => InstanceHealth::Offline,
    }
}
