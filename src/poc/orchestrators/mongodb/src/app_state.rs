//! Shared application state for all HTTP handlers and background tasks.
//!
//! Follows the Moss pattern: every field is `Arc` or cheap-to-clone.
//! Mutation goes through methods that acquire write locks.

use crate::domain::group_state::{self, GroupState, KnownMember};
use crate::domain::types::*;
use garden_common::offerings::OfferingFqn;
use orchestrator_common::events::DashboardEvent;
use orchestrator_common::persistence::TendedStone;
use orchestrator_common::stone_catalog::{StoneCatalog, StoneIdentity};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio_util::sync::CancellationToken;

/// Shared state for the MongoDB Orchestrator.
#[derive(Clone)]
pub struct AppState {
    // ── Identity ──
    pub offering_name: String,

    // ── Discovery ──
    /// Koi HTTP API endpoint (mDNS, DNS, UDP bridging).
    pub koi_endpoint: String,
    /// Explicit stone override (`--stone` / `GARDEN_STONE`). Skips discovery.
    pub explicit_stone: Option<String>,
    /// Dashboard port.
    pub dashboard_port: u16,
    /// Currently tended stone (bound via discovery or explicit).
    pub tended_stone: Arc<RwLock<Option<TendedStone>>>,

    // ── Registry ──
    /// Centralized stone identity catalog — single source of truth for
    /// name/hostname/IP/endpoint resolution.
    pub catalog: Arc<RwLock<StoneCatalog>>,
    /// All discovered MongoDB instances, keyed by stone_name.
    pub instances: Arc<RwLock<HashMap<String, MongoInstance>>>,
    /// Replica set states, keyed by FQN (e.g. `mongodb`, `mongodb::analytics`).
    pub replica_sets: Arc<RwLock<HashMap<OfferingFqn, ReplicaSetState>>>,
    /// Pending membership actions (persisted across restarts).
    pub pending_actions: Arc<RwLock<Vec<PendingAction>>>,
    /// Per-FQN group state (persisted across restarts).
    pub groups: Arc<RwLock<HashMap<String, GroupState>>>,

    // ── Signals ──
    /// Wake the conductor when the instance registry changes.
    pub conductor_notify: Arc<Notify>,

    // ── Events ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,
    pub start_time: Instant,
    pub data_dir: String,
}

impl AppState {
    pub fn new(
        offering_name: String,
        koi_endpoint: String,
        explicit_stone: Option<String>,
        dashboard_port: u16,
        data_dir: String,
        shutdown: CancellationToken,
    ) -> Self {
        let (dashboard_tx, _) = broadcast::channel(garden_common::constants::channels::SSE_DASHBOARD);

        Self {
            offering_name,
            koi_endpoint,
            explicit_stone,
            dashboard_port,
            tended_stone: Arc::new(RwLock::new(None)),
            catalog: Arc::new(RwLock::new(StoneCatalog::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            replica_sets: Arc::new(RwLock::new(HashMap::new())),
            pending_actions: Arc::new(RwLock::new(Vec::new())),
            groups: Arc::new(RwLock::new(HashMap::new())),
            conductor_notify: Arc::new(Notify::new()),
            dashboard_tx,
            shutdown,
            start_time: Instant::now(),
            data_dir,
        }
    }

    // ── Instance Management ──────────────────────────────────────

    /// Register or update a MongoDB instance. Emits a registry event.
    ///
    /// If the instance already exists, only discovery-sourced fields are updated
    /// (stone name, moss endpoint, FQN, last_seen). Health and role are preserved
    /// to avoid overwriting values set by the health monitor.
    ///
    /// Instances are keyed by `stone_name` — the catalog resolves all endpoint
    /// variants to the canonical stone identity.
    ///
    /// Returns `true` if this is a newly discovered instance, `false` if updated.
    ///
    /// When an existing instance's `mongo_endpoint` changes (IP drift from DHCP),
    /// the RS member list is updated inline so downstream logic (bootstrap,
    /// health monitor) sees the correct endpoints immediately, and an async
    /// `replSetReconfig` is triggered to update MongoDB itself.
    pub async fn upsert_instance(&self, instance: MongoInstance) -> bool {
        let key = instance.stone_name.clone();
        let is_new;
        let mut ip_drift: Option<(String, String, String)> = None; // (stone, old_ep, new_ep)
        {
            let mut reg = self.instances.write().await;
            if let Some(existing) = reg.get_mut(&key) {
                // Detect IP drift: same stone, different mongo endpoint
                if existing.mongo_endpoint != instance.mongo_endpoint {
                    ip_drift = Some((
                        key.clone(),
                        existing.mongo_endpoint.clone(),
                        instance.mongo_endpoint.clone(),
                    ));
                }
                // Merge: update discovery fields, preserve health/role/version
                existing.stone_id = instance.stone_id;
                existing.stone_name = instance.stone_name;
                existing.moss_endpoint = instance.moss_endpoint;
                existing.mongo_endpoint = instance.mongo_endpoint;
                existing.fqn = instance.fqn;
                existing.last_seen = instance.last_seen;
                // Preserve version info populated by probe_sweep (discovery has None)
                if instance.server_version.is_some() {
                    existing.server_version = instance.server_version;
                    existing.wire_version_range = instance.wire_version_range;
                }
                // Offline means the stone was unreachable — the only state
                // probe_sweep skips.  Re-discovery proves the stone is back,
                // so reset to Unknown so the next probe picks it up.
                if matches!(existing.health, InstanceHealth::Offline) {
                    existing.health = InstanceHealth::Unknown;
                }
                is_new = false;
            } else {
                reg.insert(key, instance);
                is_new = true;
            }
        }

        if let Some((stone_name, old_ep, new_ep)) = ip_drift {
            tracing::warn!(
                stone = %stone_name,
                old_endpoint = %old_ep,
                new_endpoint = %new_ep,
                "IP drift detected — endpoint changed for existing instance"
            );

            // Update the RS member list inline so bootstrap/health monitor
            // see the correct endpoint immediately (no stale-IP window).
            self.apply_endpoint_drift(&stone_name, &old_ep, &new_ep).await;

            self.emit_event(
                "instance.ip_drift",
                &serde_json::json!({
                    "stone": stone_name,
                    "old_endpoint": old_ep,
                    "new_endpoint": new_ep,
                })
                .to_string(),
            )
            .await;
        }

        if is_new {
            self.emit_event("registry.updated", "{}").await;
        }
        is_new
    }

    /// Update RS member lists and connection strings when a stone's endpoint changes.
    ///
    /// Walks all replica sets and replaces `old_ep` with `new_ep` in member lists,
    /// keeping the rest of the member state (role, health, lag) intact.
    async fn apply_endpoint_drift(&self, stone_name: &str, old_ep: &str, new_ep: &str) {
        let mut rs_map = self.replica_sets.write().await;
        for (fqn, rs) in rs_map.iter_mut() {
            let mut changed = false;
            for member in &mut rs.members {
                if member.stone_name == stone_name || member.endpoint == old_ep {
                    tracing::info!(
                        fqn = %fqn,
                        stone = %stone_name,
                        old = %member.endpoint,
                        new = %new_ep,
                        "updating RS member endpoint (IP drift)"
                    );
                    member.endpoint = new_ep.to_string();
                    changed = true;
                }
            }
            if changed {
                // Rebuild connection string with updated endpoints
                rs.connection_string =
                    Some(build_connection_string(&rs.members, &rs.rs_name));
            }
        }
    }

    /// Remove an instance from the registry by stone_name.
    pub async fn remove_instance(&self, stone_name: &str) {
        {
            let mut reg = self.instances.write().await;
            reg.remove(stone_name);
        }
        {
            let mut cat = self.catalog.write().await;
            cat.remove(stone_name);
        }
        self.emit_event("registry.updated", "{}").await;
    }

    /// Get all instances for a specific FQN.
    pub async fn instances_for_fqn(&self, fqn: &OfferingFqn) -> Vec<MongoInstance> {
        let reg = self.instances.read().await;
        reg.values()
            .filter(|i| i.fqn == *fqn)
            .cloned()
            .collect()
    }

    /// Get all distinct FQNs from registered instances.
    pub async fn distinct_fqns(&self) -> Vec<OfferingFqn> {
        let reg = self.instances.read().await;
        let mut fqns: Vec<OfferingFqn> = reg.values().map(|i| i.fqn.clone()).collect();
        fqns.sort_by_key(|a| a.to_string());
        fqns.dedup();
        fqns
    }

    /// Resolve any endpoint string to a stone_name via the catalog.
    pub async fn resolve_endpoint(&self, endpoint: &str) -> Option<String> {
        let cat = self.catalog.read().await;
        cat.resolve_name(endpoint).map(|s| s.to_string())
    }

    /// Register a stone identity in the catalog.
    pub async fn upsert_catalog(&self, identity: StoneIdentity) {
        tracing::debug!(
            stone_name = %identity.stone_name,
            hostname = %identity.hostname,
            ip = ?identity.ip,
            services = ?identity.services.iter().map(|(k, v)| (format!("{:?}", k), v.as_str())).collect::<Vec<_>>(),
            "catalog upsert"
        );
        let mut cat = self.catalog.write().await;
        cat.upsert(identity);
    }

    // ── Pending Actions ─────────────────────────────────────────

    /// Queue a pending action. Persists to disk.
    pub async fn queue_action(&self, action: PendingAction) {
        {
            let mut actions = self.pending_actions.write().await;
            actions.push(action);
            save_pending_actions(&self.data_dir, &actions).await;
        }
        self.emit_event("actions.updated", "{}").await;
    }

    /// Remove a completed action by target endpoint. Persists to disk.
    pub async fn complete_action(&self, target_endpoint: &str) {
        let mut actions = self.pending_actions.write().await;
        actions.retain(|a| a.target_endpoint() != target_endpoint);
        save_pending_actions(&self.data_dir, &actions).await;
    }

    /// Check if there is a pending removal action for a given endpoint.
    pub async fn has_pending_removal(&self, mongo_endpoint: &str) -> bool {
        let actions = self.pending_actions.read().await;
        actions.iter().any(|a| matches!(a,
            PendingAction::RemoveMember { mongo_endpoint: ep, .. } if ep == mongo_endpoint
        ))
    }

    /// Check if there is a pending removal for a specific endpoint AND FQN.
    ///
    /// Unlike `has_pending_removal`, this won't suppress discovery of the same
    /// endpoint under a *different* FQN (e.g. after reassignment).
    pub async fn has_pending_removal_for_fqn(
        &self,
        mongo_endpoint: &str,
        fqn: &OfferingFqn,
    ) -> bool {
        let actions = self.pending_actions.read().await;
        actions.iter().any(|a| matches!(a,
            PendingAction::RemoveMember { mongo_endpoint: ep, fqn: f, .. }
                if ep == mongo_endpoint && f == fqn
        ))
    }

    /// Load persisted pending actions from the data directory.
    pub async fn load_pending_actions(&self) {
        let actions = load_pending_actions(&self.data_dir).await;
        if !actions.is_empty() {
            tracing::info!(count = actions.len(), "restored pending actions from disk");
            *self.pending_actions.write().await = actions;
        }
    }

    /// Get a snapshot of all pending actions.
    pub async fn pending_actions_snapshot(&self) -> Vec<PendingAction> {
        self.pending_actions.read().await.clone()
    }

    // ── Replica Set State ────────────────────────────────────────

    /// Update the replica set state for a given FQN.
    pub async fn update_replica_set(&self, fqn: &OfferingFqn, state: ReplicaSetState) {
        let mut rs_map = self.replica_sets.write().await;
        rs_map.insert(fqn.clone(), state);
    }

    /// Get the replica set state for a given FQN.
    pub async fn replica_set_for(&self, fqn: &OfferingFqn) -> Option<ReplicaSetState> {
        let rs_map = self.replica_sets.read().await;
        rs_map.get(fqn).cloned()
    }

    // ── Group State (Persisted) ─────────────────────────────────

    /// Update the group state for a given FQN. Persists to disk.
    pub async fn update_group(&self, fqn: &OfferingFqn, state: GroupState) {
        let mut groups = self.groups.write().await;
        groups.insert(fqn.fqn(), state);
        group_state::save_groups(&self.data_dir, &groups).await;
    }

    /// Get the group state for a given FQN.
    pub async fn group_for(&self, fqn: &OfferingFqn) -> Option<GroupState> {
        let groups = self.groups.read().await;
        groups.get(&fqn.fqn()).cloned()
    }

    /// Update the known members for a group from a successful RS probe.
    ///
    /// Called by the health monitor after `rs.status()` succeeds, so the
    /// persisted state stays current for drift detection on restart.
    pub async fn update_group_members(&self, fqn: &OfferingFqn, rs_state: &ReplicaSetState) {
        let mut groups = self.groups.write().await;
        let key = fqn.fqn();
        let group = groups.entry(key).or_insert_with(|| GroupState {
            rs_name: rs_state.rs_name.clone(),
            phase: group_state::GroupPhase::Healthy,
            known_members: vec![],
            last_updated: chrono::Utc::now(),
        });

        // Update known members from the RS member list.
        // We don't have member _ids from rs.status() — those come from rs.conf().
        // Preserve existing _ids if the stone_name matches.
        let existing_ids: HashMap<String, i32> = group
            .known_members
            .iter()
            .map(|km| (km.stone_name.clone(), km.member_id))
            .collect();

        group.known_members = rs_state
            .members
            .iter()
            .map(|m| KnownMember {
                stone_name: m.stone_name.clone(),
                endpoint: m.endpoint.clone(),
                member_id: existing_ids.get(&m.stone_name).copied().unwrap_or(-1),
            })
            .collect();
        group.phase = group_state::GroupPhase::Healthy;
        group.last_updated = chrono::Utc::now();

        group_state::save_groups(&self.data_dir, &groups).await;
    }

    /// Load persisted group states from disk.
    pub async fn load_groups(&self) {
        let groups = group_state::load_groups(&self.data_dir).await;
        if !groups.is_empty() {
            tracing::info!(count = groups.len(), "restored group states from disk");
            *self.groups.write().await = groups;
        }
    }

    // ── Events ───────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }

    // ── Tending ──────────────────────────────────────────────────

    /// Bind to a stone. Persists tending state to the data directory.
    pub async fn tend_to(&self, stone: TendedStone) {
        tracing::info!(
            stone = %stone.stone_name,
            endpoint = %stone.endpoint,
            "tending to stone"
        );

        if let Err(e) =
            orchestrator_common::persistence::save_tending(&self.data_dir, &stone).await
        {
            tracing::warn!(error = %e, "failed to persist tending state");
        }

        *self.tended_stone.write().await = Some(stone);
        self.emit_event("tending.changed", "{}").await;
    }

    /// Clear tending state.
    pub async fn clear_tending(&self) {
        tracing::info!("clearing tending state");
        *self.tended_stone.write().await = None;
        orchestrator_common::persistence::clear_tending(&self.data_dir).await;
        self.emit_event("tending.changed", "{}").await;
    }

    /// Get the current tended stone's endpoint, if any.
    pub async fn tended_endpoint(&self) -> Option<String> {
        self.tended_stone
            .read()
            .await
            .as_ref()
            .map(|s| s.endpoint.clone())
    }

    /// Load persisted tending state from the data directory.
    pub async fn load_tending(&self) {
        if let Some(stone) = orchestrator_common::persistence::load_tending(&self.data_dir).await {
            tracing::info!(
                stone = %stone.stone_name,
                endpoint = %stone.endpoint,
                "restored tending state from disk"
            );
            *self.tended_stone.write().await = Some(stone);
        }
    }
}
