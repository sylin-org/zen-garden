//! Shared application state for all HTTP handlers and background tasks.
//!
//! Follows the Moss pattern: every field is `Arc` or cheap-to-clone.
//! Mutation goes through methods that acquire write locks.

use crate::catalog::OfferingRegistry;
use crate::domain::advisor::TopologyAdvice;
use crate::domain::demand::DemandLedger;
use crate::domain::fitness::BenchmarkRun;
use crate::domain::lease::LeaseManager;
use crate::domain::metrics::MetricsEngine;
use crate::domain::{recommendation, tiering};
use crate::domain::types::*;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

pub use orchestrator_common::events::DashboardEvent;
pub use orchestrator_common::persistence::TendedStone;

/// Shared state for the AI Orchestrator.
#[derive(Clone)]
pub struct AppState {
    // ── Discovery ──
    pub koi_endpoint: String,
    pub explicit_stone: Option<String>,
    pub dashboard_port: u16,
    pub tended_stone: Arc<RwLock<Option<TendedStone>>>,

    // ── Offering Registry ──
    pub registry: Arc<OfferingRegistry>,

    // ── Instance Registry ──
    pub instances: Arc<RwLock<HashMap<String, ServiceInstance>>>,
    pub models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    pub tiers: Arc<RwLock<Vec<Tier>>>,

    // ── Routing ──
    pub leases: Arc<RwLock<LeaseManager>>,
    pub queue_depths: Arc<RwLock<HashMap<String, Arc<AtomicU32>>>>,

    // ── Configuration ──
    pub config: Arc<RwLock<OrchestratorConfig>>,

    // ── Metrics ──
    pub metrics: Arc<RwLock<MetricsEngine>>,

    // ── Demand Ledger (ORCH-0009) ──
    pub demand_ledger: Arc<RwLock<DemandLedger>>,

    // ── Events ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,

    // ── Jobs ──
    pub jobs: Arc<RwLock<VecDeque<OrchestratorJob>>>,

    // ── Metric Events ──
    pub metrics_tx: mpsc::UnboundedSender<MetricEvent>,

    // ── Placement ──
    pub placement: Arc<RwLock<PlacementPlan>>,

    // ── Advisor ──
    pub advisor: Arc<RwLock<TopologyAdvice>>,

    // ── Recommendations (ORCH-0011) ──
    pub recommended_models: Arc<RwLock<HashMap<String, String>>>,

    // ── Fitness ──
    pub benchmark_run: Arc<RwLock<BenchmarkRun>>,
    pub benchmark_cancel: Arc<RwLock<Option<CancellationToken>>>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,
    pub start_time: Instant,
    pub data_dir: String,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        koi_endpoint: String,
        explicit_stone: Option<String>,
        dashboard_port: u16,
        data_dir: String,
        config: OrchestratorConfig,
        registry: OfferingRegistry,
        shutdown: CancellationToken,
        metrics_tx: mpsc::UnboundedSender<MetricEvent>,
    ) -> Self {
        let (dashboard_tx, _) =
            broadcast::channel(garden_common::constants::channels::SSE_DASHBOARD);
        let metrics_enabled = config.features.metrics_enabled;
        let mut engine = MetricsEngine::new();
        engine.enabled = metrics_enabled;

        Self {
            koi_endpoint,
            explicit_stone,
            dashboard_port,
            tended_stone: Arc::new(RwLock::new(None)),
            registry: Arc::new(registry),
            instances: Arc::new(RwLock::new(HashMap::new())),
            models: Arc::new(RwLock::new(HashMap::new())),
            tiers: Arc::new(RwLock::new(Vec::new())),
            leases: Arc::new(RwLock::new(LeaseManager::new())),
            queue_depths: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(RwLock::new(engine)),
            demand_ledger: Arc::new(RwLock::new(DemandLedger::new())),
            dashboard_tx,
            jobs: Arc::new(RwLock::new(VecDeque::with_capacity(20))),
            metrics_tx,
            placement: Arc::new(RwLock::new(PlacementPlan::default())),
            advisor: Arc::new(RwLock::new(TopologyAdvice::empty())),
            recommended_models: Arc::new(RwLock::new(HashMap::new())),
            benchmark_run: Arc::new(RwLock::new(BenchmarkRun::idle())),
            benchmark_cancel: Arc::new(RwLock::new(None)),
            shutdown,
            start_time: Instant::now(),
            data_dir,
        }
    }

    // ── Instance Management ──────────────────────────────────────

    /// Register or update a service instance. Recomputes tiers afterward.
    ///
    /// If an existing instance for the same stone (matched by `stone.id` or
    /// `stone.name`) is registered under a different endpoint — e.g. after a
    /// DHCP lease change — the stale entry is evicted first.
    pub async fn upsert_instance(&self, mut instance: ServiceInstance) {
        let endpoint = instance.endpoint.clone();
        let stone_name = instance.stone.name.clone();
        let stone_id = instance.stone.id.clone();

        // Ensure queue-depth counter exists
        {
            let mut depths = self.queue_depths.write().await;
            depths
                .entry(endpoint.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)));
        }

        // Evict stale instance for the same stone at a different endpoint
        let stale_endpoint = {
            let reg = self.instances.read().await;
            reg.iter()
                .find(|(ep, existing)| {
                    *ep != &endpoint
                        && (existing.stone.name == stone_name
                            || (!stone_id.is_empty()
                                && !existing.stone.id.is_empty()
                                && existing.stone.id == stone_id))
                })
                .map(|(ep, _)| ep.clone())
        };
        if let Some(ref stale_ep) = stale_endpoint {
            tracing::info!(
                stone = %stone_name,
                old_endpoint = %stale_ep,
                new_endpoint = %endpoint,
                "evicting stale instance (IP changed)"
            );
            let mut depths = self.queue_depths.write().await;
            depths.remove(stale_ep);
        }

        {
            let mut reg = self.instances.write().await;

            // Preserve known HW data when the incoming instance has zeroes.
            let donor = stale_endpoint
                .as_ref()
                .and_then(|ep| reg.remove(ep))
                .or_else(|| reg.get(&endpoint).cloned());

            if let Some(existing) = donor {
                if instance.vram.total_bytes == 0 && existing.vram.total_bytes > 0 {
                    instance.vram.total_bytes = existing.vram.total_bytes;
                    instance.vram.budget_bytes = existing.vram.budget_bytes;
                }
                if instance.gpu.name.is_none() && existing.gpu.name.is_some() {
                    instance.gpu.name = existing.gpu.name;
                }
            }

            reg.insert(endpoint.clone(), instance);
        }

        self.recompute_tiers().await;
        self.refresh_recommendations().await;
        self.emit_event("registry.updated", "{}").await;
    }

    /// Remove an instance from the registry.
    pub async fn remove_instance(&self, endpoint: &str) {
        {
            let mut reg = self.instances.write().await;
            reg.remove(endpoint);
        }
        self.recompute_tiers().await;
        self.refresh_recommendations().await;
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
        {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                inst.models_available = models_available;
                inst.models_loaded = models_loaded;
            }
        }
        self.refresh_recommendations().await;
    }

    /// Merge hardware data into an existing instance (partial update).
    pub async fn update_instance_hw(
        &self,
        endpoint: &str,
        vram_total_bytes: u64,
        gpu_name: Option<String>,
    ) {
        let stone_name = {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                let mut changed = false;
                if vram_total_bytes > 0 && inst.vram.total_bytes != vram_total_bytes {
                    inst.vram.total_bytes = vram_total_bytes;
                    changed = true;
                }
                if let Some(ref name) = gpu_name {
                    if inst.gpu.name.as_deref() != Some(name) {
                        inst.gpu.name = Some(name.clone());
                        changed = true;
                    }
                }
                if changed {
                    Some(inst.stone.name.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(stone_name) = stone_name {
            let budget = self.vram_budget_for(&stone_name, vram_total_bytes).await;
            {
                let mut reg = self.instances.write().await;
                if let Some(inst) = reg.get_mut(endpoint) {
                    inst.vram.budget_bytes = budget;
                }
            }
            self.recompute_tiers().await;
            self.emit_event("registry.updated", "{}").await;
        }
    }

    // ── Model Registry ───────────────────────────────────────────

    pub async fn upsert_model(&self, info: ModelInfo) {
        {
            let mut models = self.models.write().await;
            models.insert(info.name.clone(), info);
        }
        self.refresh_recommendations().await;
    }

    pub async fn remove_model(&self, name: &str) {
        {
            let mut models = self.models.write().await;
            models.remove(name);
        }
        self.refresh_recommendations().await;
    }

    // ── Tier Recomputation ───────────────────────────────────────

    async fn recompute_tiers(&self) {
        let instances = self.instances.read().await;
        let new_tiers = tiering::compute_tiers(&instances);
        let mut tiers = self.tiers.write().await;
        *tiers = new_tiers;
    }

    // ── Queue Depth ──────────────────────────────────────────────

    pub async fn queue_counter(&self, endpoint: &str) -> Arc<AtomicU32> {
        let depths = self.queue_depths.read().await;
        depths
            .get(endpoint)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicU32::new(0)))
    }

    // ── Events ───────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }

    // ── Jobs ─────────────────────────────────────────────────────

    const MAX_JOBS: usize = 20;

    pub async fn create_job(&self, kind: JobKind) -> String {
        let id = format!("job-{}", chrono::Utc::now().timestamp_millis());
        let job = OrchestratorJob {
            id: id.clone(),
            kind: kind.clone(),
            status: JobStatus::Queued,
            progress: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            error: None,
        };

        let mut jobs = self.jobs.write().await;
        if jobs.len() >= Self::MAX_JOBS {
            jobs.pop_front();
        }
        jobs.push_back(job);

        self.emit_event(
            "job.created",
            &serde_json::json!({"id": &id, "kind": kind.label(), "subject": kind.subject()})
                .to_string(),
        )
        .await;
        id
    }

    pub async fn update_job(&self, id: &str, status: JobStatus, progress: Option<String>) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = status;
            if progress.is_some() {
                job.progress = progress;
            }
        }
    }

    pub async fn complete_job(&self, id: &str) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Completed;
            job.completed_at = Some(chrono::Utc::now());
        }
        drop(jobs);
        self.emit_event("job.completed", &serde_json::json!({"id": id}).to_string())
            .await;
    }

    pub async fn fail_job(&self, id: &str, error: &str) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Failed;
            job.completed_at = Some(chrono::Utc::now());
            job.error = Some(error.to_string());
        }
        drop(jobs);
        self.emit_event(
            "job.failed",
            &serde_json::json!({"id": id, "error": error}).to_string(),
        )
        .await;
    }

    // ── Config ───────────────────────────────────────────────────

    pub async fn vram_budget_for(&self, stone_name: &str, vram_total: u64) -> u64 {
        let config = self.config.read().await;
        config
            .stones
            .get(stone_name)
            .and_then(|s| s.vram_budget_mb)
            .map(|mb| mb * 1_048_576)
            .unwrap_or(vram_total)
    }

    // ── Recommendations ────────────────────────────────────────

    /// All user-facing recommendation capabilities (extended for multi-offering).
    const ALL_CAPABILITIES: &[&str] = &[
        "quick",
        "chat",
        "synthesis",
        "vision",
        "ocr",
        "tools",
        "thinking",
        "embedding",
        "imagine",
        "transcribe",
        "speak",
        "rerank",
        "translate",
    ];

    pub async fn refresh_recommendations(&self) {
        let models = self.models.read().await.clone();
        let instances = self.instances.read().await.clone();
        let gpu_matrix = self.benchmark_run.read().await.gpu_matrix.clone();
        let pins = self.config.read().await.features.pins.clone();

        let mut cache = HashMap::with_capacity(Self::ALL_CAPABILITIES.len());
        for &cap in Self::ALL_CAPABILITIES {
            let pin = pins.get(cap).map(|s| s.as_str());
            let resp = recommendation::recommend(cap, &models, &instances, &gpu_matrix, pin);
            if let Some(selected) = resp.selected {
                cache.insert(cap.to_string(), selected);
            }
        }

        *self.recommended_models.write().await = cache;
    }

    // ── Tending ──────────────────────────────────────────────────

    pub async fn tend_to(&self, stone: TendedStone) {
        tracing::info!(
            stone = %stone.stone_name,
            endpoint = %stone.endpoint,
            "tending to stone"
        );

        let path = std::path::Path::new(&self.data_dir).join(".tending");
        if let Ok(json) = serde_json::to_string_pretty(&stone) {
            let _ = tokio::fs::write(&path, json).await;
        }

        *self.tended_stone.write().await = Some(stone);
        self.emit_event("tending.changed", "{}").await;
    }

    pub async fn clear_tending(&self) {
        tracing::info!("clearing tending state");
        *self.tended_stone.write().await = None;

        let path = std::path::Path::new(&self.data_dir).join(".tending");
        let _ = tokio::fs::remove_file(&path).await;
        self.emit_event("tending.changed", "{}").await;
    }

    pub async fn tended_endpoint(&self) -> Option<String> {
        self.tended_stone
            .read()
            .await
            .as_ref()
            .map(|s| s.endpoint.clone())
    }

    pub async fn load_tending(&self) {
        let path = std::path::Path::new(&self.data_dir).join(".tending");
        if let Ok(data) = tokio::fs::read_to_string(&path).await {
            if let Ok(stone) = serde_json::from_str::<TendedStone>(&data) {
                tracing::info!(
                    stone = %stone.stone_name,
                    endpoint = %stone.endpoint,
                    "restored tending state from disk"
                );
                *self.tended_stone.write().await = Some(stone);
            }
        }
    }
}
