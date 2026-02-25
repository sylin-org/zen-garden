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
            dashboard_tx,
            shutdown,
            start_time: Instant::now(),
            data_dir,
        }
    }

    // ── Instance Management ──────────────────────────────────────

    /// Register or update a MongoDB instance. Emits a registry event.
    pub async fn upsert_instance(&self, instance: MongoInstance) {
        let endpoint = instance.mongo_endpoint.clone();
        {
            let mut reg = self.instances.write().await;
            reg.insert(endpoint, instance);
        }
        self.emit_event("registry.updated", "{}").await;
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
