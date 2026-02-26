//! Shared application state for all HTTP handlers and background tasks.
//!
//! Follows the Moss pattern: every field is `Arc` or cheap-to-clone.
//! Mutation goes through methods that acquire write locks.

use crate::domain::types::*;
use orchestrator_common::events::DashboardEvent;
use orchestrator_common::persistence::TendedStone;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
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
    /// All discovered MongoDB instances, keyed by mongo_endpoint.
    pub instances: Arc<RwLock<HashMap<String, MongoInstance>>>,
    /// Replica set states, keyed by FQN (e.g. "mongodb", "mongodb:analytics").
    pub replica_sets: Arc<RwLock<HashMap<String, ReplicaSetState>>>,
    /// Pending membership actions (persisted across restarts).
    pub pending_actions: Arc<RwLock<Vec<PendingAction>>>,

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
        let (dashboard_tx, _) = broadcast::channel(256);

        Self {
            offering_name,
            koi_endpoint,
            explicit_stone,
            dashboard_port,
            tended_stone: Arc::new(RwLock::new(None)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            replica_sets: Arc::new(RwLock::new(HashMap::new())),
            pending_actions: Arc::new(RwLock::new(Vec::new())),
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
    /// Upsert an instance into the registry.
    /// Returns `true` if this is a newly discovered instance, `false` if updated.
    pub async fn upsert_instance(&self, instance: MongoInstance) -> bool {
        let endpoint = instance.mongo_endpoint.clone();
        let is_new;
        {
            let mut reg = self.instances.write().await;
            if let Some(existing) = reg.get_mut(&endpoint) {
                // Merge: update discovery fields, preserve health/role
                existing.stone_id = instance.stone_id;
                existing.stone_name = instance.stone_name;
                existing.moss_endpoint = instance.moss_endpoint;
                existing.fqn = instance.fqn;
                existing.last_seen = instance.last_seen;
                is_new = false;
            } else {
                reg.insert(endpoint, instance);
                is_new = true;
            }
        }
        if is_new {
            self.emit_event("registry.updated", "{}").await;
        }
        is_new
    }

    /// Remove an instance from the registry.
    pub async fn remove_instance(&self, mongo_endpoint: &str) {
        {
            let mut reg = self.instances.write().await;
            reg.remove(mongo_endpoint);
        }
        self.emit_event("registry.updated", "{}").await;
    }

    /// Get all instances for a specific FQN.
    pub async fn instances_for_fqn(&self, fqn: &str) -> Vec<MongoInstance> {
        let reg = self.instances.read().await;
        reg.values()
            .filter(|i| i.fqn == fqn)
            .cloned()
            .collect()
    }

    /// Get all distinct FQNs from registered instances.
    pub async fn distinct_fqns(&self) -> Vec<String> {
        let reg = self.instances.read().await;
        let mut fqns: Vec<String> = reg.values().map(|i| i.fqn.clone()).collect();
        fqns.sort();
        fqns.dedup();
        fqns
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
    pub async fn update_replica_set(&self, fqn: &str, state: ReplicaSetState) {
        let mut rs_map = self.replica_sets.write().await;
        rs_map.insert(fqn.to_string(), state);
    }

    /// Get the replica set state for a given FQN.
    pub async fn replica_set_for(&self, fqn: &str) -> Option<ReplicaSetState> {
        let rs_map = self.replica_sets.read().await;
        rs_map.get(fqn).cloned()
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
