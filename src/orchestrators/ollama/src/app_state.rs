//! Shared application state for all HTTP handlers and background tasks.
//!
//! Follows the Moss pattern: every field is `Arc` or cheap-to-clone.
//! Mutation goes through methods that acquire write locks.

use crate::domain::advisor::TopologyAdvice;
use crate::domain::demand::DemandLedger;
use crate::domain::fitness::BenchmarkRun;
use crate::domain::lease::LeaseManager;
use crate::domain::metrics::MetricsEngine;
use crate::domain::{recommendation, tiering};
use crate::domain::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tokio_util::sync::CancellationToken;

/// Dashboard SSE event.
#[derive(Debug, Clone)]
pub struct DashboardEvent {
    pub event_type: String,
    pub data: String,
}

/// Persisted tending state — which stone we're bound to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendedStone {
    pub stone_name: String,
    pub stone_id: Option<String>,
    pub endpoint: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// Shared state for the Ollama Orchestrator.
#[derive(Clone)]
pub struct AppState {
    // ── Identity ──
    pub offering_name: String,

    // ── Discovery ──
    /// Koi HTTP API endpoint (mDNS, DNS, UDP bridging).
    pub koi_endpoint: String,
    /// Explicit stone override (`--stone` / `GARDEN_STONE`). Skips discovery.
    pub explicit_stone: Option<String>,
    /// Proxy port (Ollama-compatible endpoint, e.g. 21434).
    pub proxy_port: u16,
    /// Dashboard port (management UI, e.g. 7190).
    pub dashboard_port: u16,
    /// Currently tended stone (bound via discovery or explicit).
    pub tended_stone: Arc<RwLock<Option<TendedStone>>>,

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

    // ── Demand Ledger (ORCH-0009) ──
    /// Exponentially decayed demand tracking for the topology advisor.
    pub demand_ledger: Arc<RwLock<DemandLedger>>,

    // ── Events ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,

    // ── Jobs ──
    pub jobs: Arc<RwLock<VecDeque<OrchestratorJob>>>,

    // ── Shared Snapshot Space ──
    /// Pre-built dashboard JSON (written by snapshot_publisher, read by dashboard).
    pub snapshot_rx: watch::Receiver<serde_json::Value>,

    // ── Metric Events ──
    /// Fire-and-forget channel: proxy → metrics processor.
    pub metrics_tx: mpsc::UnboundedSender<MetricEvent>,

    // ── Placement ──
    pub placement: Arc<RwLock<PlacementPlan>>,

    // ── Advisor ──
    /// Topology recommendation (cold T=0 + periodic refresh).
    pub advisor: Arc<RwLock<TopologyAdvice>>,

    // ── Recommendations (ORCH-0011) ──
    /// Cached moniker resolution: capability → selected model name.
    /// Refreshed on model/instance/benchmark/pin changes.
    pub recommended_models: Arc<RwLock<HashMap<String, String>>>,

    // ── Fitness ──
    /// Full benchmark run state (tree: run → stones → tests → samples).
    pub benchmark_run: Arc<RwLock<BenchmarkRun>>,
    /// Cancel token for a running benchmark (None when idle).
    pub benchmark_cancel: Arc<RwLock<Option<CancellationToken>>>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,
    pub start_time: Instant,
    pub data_dir: String,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        offering_name: String,
        koi_endpoint: String,
        explicit_stone: Option<String>,
        proxy_port: u16,
        dashboard_port: u16,
        data_dir: String,
        config: RouterConfig,
        shutdown: CancellationToken,
        snapshot_rx: watch::Receiver<serde_json::Value>,
        metrics_tx: mpsc::UnboundedSender<MetricEvent>,
    ) -> Self {
        let (dashboard_tx, _) = broadcast::channel(256);
        let metrics_enabled = config.features.metrics_enabled;
        let mut engine = MetricsEngine::new();
        engine.enabled = metrics_enabled;

        Self {
            offering_name,
            koi_endpoint,
            explicit_stone,
            proxy_port,
            dashboard_port,
            tended_stone: Arc::new(RwLock::new(None)),
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
            snapshot_rx,
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

    /// Register or update an Ollama instance. Recomputes tiers afterward.
    ///
    /// If an existing instance for the same stone (matched by `stone_id` or
    /// `stone_name`) is registered under a different endpoint — e.g. after a
    /// DHCP lease change — the stale entry is evicted first.
    pub async fn upsert_instance(&self, mut instance: OllamaInstance) {
        let endpoint = instance.endpoint.clone();
        let stone_name = instance.stone_name.clone();
        let stone_id = instance.stone_id.clone();

        // Ensure queue-depth counter exists
        {
            let mut depths = self.queue_depths.write().await;
            depths
                .entry(endpoint.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)));
        }

        // Evict any existing instance for the same stone but at a different
        // endpoint (e.g. stone rebooted and received a new IP from DHCP).
        let stale_endpoint = {
            let reg = self.instances.read().await;
            reg.iter()
                .find(|(ep, existing)| {
                    *ep != &endpoint
                        && (existing.stone_name == stone_name
                            || (!stone_id.is_empty()
                                && !existing.stone_id.is_empty()
                                && existing.stone_id == stone_id))
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
            // This prevents the SSE re-discovery path (which may fetch HW
            // before Moss has GPU data ready) from overwriting good values
            // that were set by the topology query.
            let donor = stale_endpoint
                .as_ref()
                .and_then(|ep| reg.remove(ep))
                .or_else(|| reg.get(&endpoint).cloned());

            if let Some(existing) = donor {
                if instance.vram_total_bytes == 0 && existing.vram_total_bytes > 0 {
                    instance.vram_total_bytes = existing.vram_total_bytes;
                    instance.vram_budget_bytes = existing.vram_budget_bytes;
                }
                if instance.gpu_name.is_none() && existing.gpu_name.is_some() {
                    instance.gpu_name = existing.gpu_name;
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
                inst.last_profiled = Instant::now();
            }
        }
        self.refresh_recommendations().await;
    }

    /// Merge hardware data into an existing instance (partial update).
    ///
    /// Only updates VRAM and GPU name when the new values are meaningful
    /// (non-zero VRAM, non-None GPU).  Recalculates VRAM budget and
    /// recomputes tiers if anything changed.
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
                if vram_total_bytes > 0 && inst.vram_total_bytes != vram_total_bytes {
                    inst.vram_total_bytes = vram_total_bytes;
                    changed = true;
                }
                if let Some(ref name) = gpu_name {
                    if inst.gpu_name.as_deref() != Some(name) {
                        inst.gpu_name = Some(name.clone());
                        changed = true;
                    }
                }
                if changed {
                    Some(inst.stone_name.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Update budget and recompute tiers outside the write lock
        if let Some(stone_name) = stone_name {
            let budget = self.vram_budget_for(&stone_name, vram_total_bytes).await;
            {
                let mut reg = self.instances.write().await;
                if let Some(inst) = reg.get_mut(endpoint) {
                    inst.vram_budget_bytes = budget;
                }
            }
            self.recompute_tiers().await;
            self.emit_event("registry.updated", "{}").await;
        }
    }

    // ── Model Registry ───────────────────────────────────────────

    /// Add or update a model's metadata.
    pub async fn upsert_model(&self, info: ModelInfo) {
        {
            let mut models = self.models.write().await;
            models.insert(info.name.clone(), info);
        }
        self.refresh_recommendations().await;
    }

    /// Remove a model from the global registry.
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

    // ── Events ───────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }

    // ── Jobs ─────────────────────────────────────────────────────

    /// Maximum number of jobs to retain in the ring buffer.
    const MAX_JOBS: usize = 20;

    /// Create a new job, add it to the ring buffer, and return its ID.
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

    /// Mark a job as running, with optional progress text.
    pub async fn update_job(&self, id: &str, status: JobStatus, progress: Option<String>) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = status;
            if progress.is_some() {
                job.progress = progress;
            }
        }
    }

    /// Mark a job as completed.
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

    /// Mark a job as failed with an error message.
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

    // ── Recommendations ────────────────────────────────────────

    /// All user-facing recommendation capabilities.
    const ALL_CAPABILITIES: &[&str] = &[
        "quick", "chat", "synthesis", "vision", "ocr", "tools", "thinking", "embedding",
    ];

    /// Recompute the cached `recommended_models` map from current state.
    ///
    /// Called after any mutation that could change recommendation rankings:
    /// model/instance registry, benchmark results, or pin changes.
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

    /// Bind to a stone. Persists tending state to the data directory.
    pub async fn tend_to(&self, stone: TendedStone) {
        tracing::info!(
            stone = %stone.stone_name,
            endpoint = %stone.endpoint,
            "tending to stone"
        );

        // Persist to disk
        let path = std::path::Path::new(&self.data_dir).join(".tending");
        if let Ok(json) = serde_json::to_string_pretty(&stone) {
            let _ = tokio::fs::write(&path, json).await;
        }

        *self.tended_stone.write().await = Some(stone);
        self.emit_event("tending.changed", "{}").await;
    }

    /// Clear tending state.
    pub async fn clear_tending(&self) {
        tracing::info!("clearing tending state");
        *self.tended_stone.write().await = None;

        let path = std::path::Path::new(&self.data_dir).join(".tending");
        let _ = tokio::fs::remove_file(&path).await;
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
