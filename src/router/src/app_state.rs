//! Shared application state for all HTTP handlers and background tasks.
//!
//! Follows the Moss pattern: every field is `Arc` or cheap-to-clone.
//! Mutation goes through methods that acquire write locks.

use crate::domain::lease::LeaseManager;
use crate::domain::metrics::MetricsEngine;
use crate::domain::tiering;
use crate::domain::types::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

/// Dashboard SSE event.
#[derive(Debug, Clone)]
pub struct DashboardEvent {
    pub event_type: String,
    pub data: String,
}

/// Shared state for the AI Router.
#[derive(Clone)]
pub struct AppState {
    // ── Identity ──
    pub offering_name: String,
    pub stone_endpoint: String,

    // ── Registry ──
    pub instances: Arc<RwLock<HashMap<String, OllamaInstance>>>,
    pub models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    pub tiers: Arc<RwLock<Vec<Tier>>>,

    // ── Routing ──
    pub leases: Arc<RwLock<LeaseManager>>,
    /// Per-endpoint atomic queue depth counters.
    pub queue_depths: Arc<RwLock<HashMap<String, Arc<AtomicU32>>>>,

    // ── Configuration ──
    pub config: Arc<RwLock<RouterConfig>>,

    // ── Metrics ──
    pub metrics: Arc<RwLock<MetricsEngine>>,

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
        stone_endpoint: String,
        data_dir: String,
        config: RouterConfig,
        shutdown: CancellationToken,
    ) -> Self {
        let (dashboard_tx, _) = broadcast::channel(256);
        let metrics_enabled = config.features.metrics_enabled;
        let mut engine = MetricsEngine::new();
        engine.enabled = metrics_enabled;

        Self {
            offering_name,
            stone_endpoint,
            instances: Arc::new(RwLock::new(HashMap::new())),
            models: Arc::new(RwLock::new(HashMap::new())),
            tiers: Arc::new(RwLock::new(Vec::new())),
            leases: Arc::new(RwLock::new(LeaseManager::new())),
            queue_depths: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(RwLock::new(engine)),
            dashboard_tx,
            shutdown,
            start_time: Instant::now(),
            data_dir,
        }
    }

    // ── Instance Management ──────────────────────────────────────

    /// Register or update an Ollama instance. Recomputes tiers afterward.
    pub async fn upsert_instance(&self, instance: OllamaInstance) {
        let endpoint = instance.endpoint.clone();

        // Ensure queue-depth counter exists
        {
            let mut depths = self.queue_depths.write().await;
            depths
                .entry(endpoint.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)));
        }

        {
            let mut reg = self.instances.write().await;
            reg.insert(endpoint.clone(), instance);
        }

        self.recompute_tiers().await;
        self.emit_event("registry.updated", "{}").await;
    }

    /// Remove an instance from the registry.
    pub async fn remove_instance(&self, endpoint: &str) {
        {
            let mut reg = self.instances.write().await;
            reg.remove(endpoint);
        }
        self.recompute_tiers().await;
        self.emit_event("registry.updated", "{}").await;
    }

    /// Mark instance healthy/unhealthy.
    pub async fn set_instance_health(&self, endpoint: &str, health: InstanceHealth) {
        let mut changed = false;
        {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                if inst.health != health {
                    inst.health = health;
                    changed = true;
                }
            }
        }
        if changed {
            self.recompute_tiers().await;
        }
    }

    /// Update instance model inventory and load state.
    pub async fn update_instance_models(
        &self,
        endpoint: &str,
        models_available: Vec<String>,
        models_loaded: Vec<LoadedModel>,
    ) {
        let mut reg = self.instances.write().await;
        if let Some(inst) = reg.get_mut(endpoint) {
            inst.models_available = models_available;
            inst.models_loaded = models_loaded;
            inst.last_profiled = Instant::now();
        }
    }

    // ── Model Registry ───────────────────────────────────────────

    /// Add or update a model's metadata.
    pub async fn upsert_model(&self, info: ModelInfo) {
        let mut models = self.models.write().await;
        models.insert(info.name.clone(), info);
    }

    /// Remove a model from the global registry.
    pub async fn remove_model(&self, name: &str) {
        let mut models = self.models.write().await;
        models.remove(name);
    }

    // ── Tier Recomputation ───────────────────────────────────────

    async fn recompute_tiers(&self) {
        let instances = self.instances.read().await;
        let instance_list: Vec<OllamaInstance> = instances.values().cloned().collect();
        let new_tiers = tiering::compute_tiers(&instance_list);
        let mut tiers = self.tiers.write().await;
        *tiers = new_tiers;
    }

    // ── Queue Depth ──────────────────────────────────────────────

    /// Get the atomic queue-depth counter for an endpoint.
    pub async fn queue_counter(&self, endpoint: &str) -> Arc<AtomicU32> {
        let depths = self.queue_depths.read().await;
        depths
            .get(endpoint)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicU32::new(0)))
    }

    /// Sync queue depths into the instance records (for routing decisions).
    pub async fn sync_queue_depths(&self) {
        let depths = self.queue_depths.read().await;
        let mut reg = self.instances.write().await;
        for (ep, counter) in depths.iter() {
            if let Some(inst) = reg.get_mut(ep.as_str()) {
                inst.queue_depth = counter.load(Ordering::Relaxed);
            }
        }
    }

    // ── Events ───────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }

    // ── Config ───────────────────────────────────────────────────

    /// Get the VRAM budget for a stone, or fall back to the discovered total.
    pub async fn vram_budget_for(&self, stone_name: &str, vram_total: u64) -> u64 {
        let config = self.config.read().await;
        config
            .stones
            .get(stone_name)
            .and_then(|s| s.vram_budget_mb)
            .map(|mb| mb * 1_048_576)
            .unwrap_or(vram_total)
    }
}
