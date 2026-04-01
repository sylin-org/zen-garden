//! Shared application state — thin facade over domain objects (ORCH-0020).
//!
//! Domains own their mutable state privately and publish immutable snapshots
//! via `watch`. API handlers read snapshots with zero locks.
//!
//! Delegation methods on AppState exist temporarily during migration.
//! Call sites will be updated to use domains directly, then shims removed.

use crate::catalog::ProviderRegistry;
use crate::domain::directory_domain::{DirectoryDomain, DirectorySnapshot};
use crate::domain::intelligence::{IntelligenceDomain, IntelligenceRunner, IntelligenceSnapshot};
use crate::domain::observability::{ObservabilityDomain, ObservabilitySnapshot};
use crate::domain::registry::{RegistryDomain, RegistrySnapshot};
use crate::domain::skills_domain::{SkillsDomain, SkillsSnapshot};
use crate::domain::fitness::BenchmarkRun;
use crate::domain::skill::{SkillDefinition, SkillStatus, WorkflowJob};
use crate::domain::types::*;
use crate::offerings::cloud::CloudProviderStore;
use crate::offerings::ollama::OllamaClient;
use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tokio_util::sync::CancellationToken;

pub use orchestrator_common::events::DashboardEvent;
pub use orchestrator_common::persistence::TendedStone;

/// Shared state for the AI Orchestrator.
#[derive(Clone)]
pub struct AppState {
    // ── Domains (black boxes — own mutable state, publish snapshots) ──
    pub registry: Arc<RegistryDomain>,
    pub directory: Arc<DirectoryDomain>,
    pub intelligence: Arc<IntelligenceDomain>,
    pub observability: Arc<ObservabilityDomain>,
    pub skills: Arc<SkillsDomain>,

    // ── Immutable (set at startup) ──
    pub providers: Arc<ProviderRegistry>,
    pub ollama_client: OllamaClient,
    pub koi_endpoint: String,
    pub explicit_stone: Option<String>,
    pub dashboard_port: u16,
    pub data_dir: String,
    pub start_time: Instant,

    // ── Rarely mutated (stays RwLock — user-action speed) ──
    pub config: Arc<RwLock<OrchestratorConfig>>,
    pub cloud_store: Arc<RwLock<CloudProviderStore>>,
    pub tended_stone: Arc<RwLock<Option<TendedStone>>>,
    pub benchmark_run: Arc<RwLock<BenchmarkRun>>,
    pub benchmark_cancel: Arc<RwLock<Option<CancellationToken>>>,

    // ── Channels (already lock-free) ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,
    pub metrics_tx: mpsc::UnboundedSender<MetricEvent>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,

    // ── Legacy fields (kept during migration, will be removed) ──
    // These are the old RwLock fields. Call sites that still reference them
    // will be migrated incrementally. The domains are the source of truth.
    pub instances: Arc<RwLock<HashMap<String, ServiceInstance>>>,
    pub tiers: Arc<RwLock<Vec<Tier>>>,
    pub queue_depths: Arc<RwLock<HashMap<String, Arc<AtomicU32>>>>,
    pub directory_legacy: Arc<RwLock<ModelDirectory>>,
    pub recommended_models: Arc<RwLock<HashMap<String, String>>>,
    pub skill_registry: Arc<RwLock<crate::domain::skill::SkillRegistry>>,
    pub workflow_jobs: Arc<RwLock<HashMap<String, WorkflowJob>>>,
    pub metrics: Arc<RwLock<crate::domain::metrics::MetricsEngine>>,
    pub demand_ledger: Arc<RwLock<crate::domain::demand::DemandLedger>>,
    pub jobs: Arc<RwLock<std::collections::VecDeque<OrchestratorJob>>>,
    pub leases: Arc<RwLock<crate::domain::lease::LeaseManager>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        koi_endpoint: String,
        explicit_stone: Option<String>,
        dashboard_port: u16,
        data_dir: String,
        config: OrchestratorConfig,
        providers: ProviderRegistry,
        ollama_client: OllamaClient,
        cloud_store: CloudProviderStore,
        shutdown: CancellationToken,
        metrics_tx: mpsc::UnboundedSender<MetricEvent>,
    ) -> Self {
        let (dashboard_tx, _) =
            broadcast::channel(garden_common::constants::channels::SSE_DASHBOARD);

        let metrics_enabled = config.features.metrics_enabled;

        // Create watch channels for each domain
        let (reg_tx, _) = watch::channel(Arc::new(RegistrySnapshot::empty()));
        let (dir_tx, _) = watch::channel(Arc::new(DirectorySnapshot::empty()));
        let (intel_tx, _) = watch::channel(Arc::new(IntelligenceSnapshot::empty()));
        let (obs_tx, _) = watch::channel(Arc::new(ObservabilitySnapshot::empty()));
        let (skills_tx, _) = watch::channel(Arc::new(SkillsSnapshot::empty()));

        // Build domains
        let registry = Arc::new(RegistryDomain::new(reg_tx));
        let directory = Arc::new(DirectoryDomain::new(dir_tx));
        // IntelligenceDomain uses a separate sender for borrow() reads.
        // IntelligenceRunner gets the real sender + watch receivers.
        // They share the same channel — runner sends, domain borrows.
        let intelligence = Arc::new(IntelligenceDomain::new(intel_tx.clone()));

        let observability = Arc::new(ObservabilityDomain::new(obs_tx, metrics_enabled));
        let skills = Arc::new(SkillsDomain::new(skills_tx));

        // Legacy fields — kept during migration
        let mut legacy_metrics = crate::domain::metrics::MetricsEngine::new();
        legacy_metrics.enabled = metrics_enabled;

        Self {
            // Domains
            registry,
            directory,
            intelligence,
            observability,
            skills,

            // Immutable
            providers: Arc::new(providers),
            ollama_client,
            koi_endpoint,
            explicit_stone,
            dashboard_port,
            data_dir,
            start_time: Instant::now(),

            // Rarely mutated
            config: Arc::new(RwLock::new(config)),
            cloud_store: Arc::new(RwLock::new(cloud_store)),
            tended_stone: Arc::new(RwLock::new(None)),
            benchmark_run: Arc::new(RwLock::new(BenchmarkRun::idle())),
            benchmark_cancel: Arc::new(RwLock::new(None)),

            // Channels
            dashboard_tx,
            metrics_tx,

            // Lifecycle
            shutdown,

            // Legacy (migration shims — will be removed)
            instances: Arc::new(RwLock::new(HashMap::new())),
            tiers: Arc::new(RwLock::new(Vec::new())),
            queue_depths: Arc::new(RwLock::new(HashMap::new())),
            directory_legacy: Arc::new(RwLock::new(ModelDirectory::new())),
            recommended_models: Arc::new(RwLock::new(HashMap::new())),
            skill_registry: Arc::new(RwLock::new(crate::domain::skill::SkillRegistry::new())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(legacy_metrics)),
            demand_ledger: Arc::new(RwLock::new(crate::domain::demand::DemandLedger::new())),
            jobs: Arc::new(RwLock::new(std::collections::VecDeque::with_capacity(20))),
            leases: Arc::new(RwLock::new(crate::domain::lease::LeaseManager::new())),
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Delegation methods — call both legacy RwLock AND new domain.
    // These ensure both paths stay in sync during incremental migration.
    // Once all call sites use domains directly, these will be removed.
    // ════════════════════════════════════════════════════════════════

    // ── Instance Management (delegates to Registry domain) ──────

    pub async fn upsert_instance(&self, instance: ServiceInstance) {
        let endpoint = instance.endpoint.clone();
        let stone_name = instance.stone.name.clone();
        let stone_id = instance.stone.id.clone();
        let kind = instance.kind;

        // Write to domain
        let config = self.config.read().await;
        self.registry.upsert_instance(instance.clone(), &config).await;
        drop(config);

        // Write to legacy
        {
            let mut depths = self.queue_depths.write().await;
            depths
                .entry(endpoint.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)));
        }

        let stale_endpoint = {
            let reg = self.instances.read().await;
            reg.iter()
                .find(|(ep, existing)| {
                    *ep != &endpoint
                        && existing.kind == kind
                        && (existing.stone.name == stone_name
                            || (!stone_id.is_empty()
                                && !existing.stone.id.is_empty()
                                && existing.stone.id == stone_id))
                })
                .map(|(ep, _)| ep.clone())
        };
        if let Some(ref stale_ep) = stale_endpoint {
            let mut depths = self.queue_depths.write().await;
            depths.remove(stale_ep);
        }

        {
            let mut reg = self.instances.write().await;
            if let Some(stale_ep) = &stale_endpoint {
                reg.remove(stale_ep);
            }
            // Use the domain snapshot as source of truth for the legacy map
            let snap = self.registry.snapshot();
            *reg = (*snap.instances).clone();
        }

        self.recompute_tiers().await;
        self.refresh_recommendations().await;
        self.emit_event("registry.updated", "{}").await;
    }

    pub async fn remove_instance(&self, endpoint: &str) {
        self.registry.remove_instance(endpoint).await;

        {
            let mut reg = self.instances.write().await;
            reg.remove(endpoint);
        }
        self.recompute_tiers().await;
        self.refresh_recommendations().await;
        self.emit_event("registry.updated", "{}").await;
    }

    pub async fn set_instance_health(&self, endpoint: &str, health: InstanceHealth) {
        let changed = self.registry.set_instance_health(endpoint, health.clone()).await;

        {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                inst.health = health;
            }
        }

        if changed {
            self.recompute_tiers().await;
            self.refresh_recommendations().await;
            self.emit_event("registry.updated", "{}").await;
        }
    }

    pub async fn update_instance_models(
        &self,
        endpoint: &str,
        available: Vec<String>,
        loaded: Vec<LoadedModel>,
    ) {
        self.registry
            .update_instance_models(endpoint, available.clone(), loaded.clone())
            .await;

        {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                inst.models_available = available;
                inst.models_loaded = loaded;
            }
        }
        self.refresh_recommendations().await;
        self.emit_event("registry.updated", "{}").await;
    }

    pub async fn update_instance_hw(
        &self,
        endpoint: &str,
        vram_total: u64,
        gpu_name: Option<String>,
    ) {
        let config = self.config.read().await;
        self.registry
            .update_instance_hw(endpoint, vram_total, gpu_name.clone(), &config)
            .await;
        drop(config);

        {
            let mut reg = self.instances.write().await;
            if let Some(inst) = reg.get_mut(endpoint) {
                if vram_total > 0 {
                    inst.vram.total_bytes = vram_total;
                    let config = self.config.blocking_read();
                    inst.vram.budget_bytes = config
                        .stones
                        .get(&inst.stone.name)
                        .and_then(|s| s.vram_budget_mb)
                        .map(|mb| mb * 1_048_576)
                        .unwrap_or(vram_total);
                }
                if gpu_name.is_some() {
                    inst.gpu.name = gpu_name;
                }
            }
        }
        self.recompute_tiers().await;
        self.emit_event("registry.updated", "{}").await;
    }

    async fn recompute_tiers(&self) {
        let instances = self.instances.read().await;
        let new_tiers = crate::domain::tiering::compute_tiers(&instances);
        let mut tiers = self.tiers.write().await;
        *tiers = new_tiers;
    }

    pub async fn queue_counter(&self, endpoint: &str) -> Arc<AtomicU32> {
        // Prefer domain, fall back to legacy
        self.registry.queue_counter(endpoint).await
    }

    // ── Directory (delegates to Directory domain) ───────────────

    pub async fn directory_upsert(
        &self,
        fqn: ModelFqn,
        capabilities: Vec<Capability>,
        specializations: Vec<String>,
        metadata: ModelMetadata,
    ) {
        self.directory
            .upsert(fqn.clone(), capabilities.clone(), specializations.clone(), metadata.clone())
            .await;

        // Legacy
        {
            let mut dir = self.directory_legacy.write().await;
            dir.upsert(fqn, capabilities, specializations, metadata);
        }
        self.refresh_recommendations().await;
    }

    pub async fn directory_remove_provider(&self, source: &str, locator: &str) {
        self.directory.remove_provider(source, locator).await;

        {
            let mut dir = self.directory_legacy.write().await;
            dir.remove_provider(source, locator);
        }
        self.refresh_recommendations().await;
    }

    // ── Events ──────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }

    // ── Jobs (delegates to Observability domain) ────────────────

    pub async fn create_job(&self, kind: JobKind) -> String {
        let id = self.observability.create_job(kind.clone()).await;

        // Legacy
        {
            let mut jobs = self.jobs.write().await;
            // Sync from domain snapshot
            let snap = self.observability.snapshot();
            *jobs = snap.jobs.iter().cloned().collect();
        }

        self.emit_event(
            "job.created",
            &serde_json::json!({"id": &id, "kind": kind.label(), "subject": kind.subject()})
                .to_string(),
        )
        .await;
        id
    }

    pub async fn update_job(&self, id: &str, status: JobStatus, progress: Option<String>) {
        self.observability.update_job(id, status, progress).await;

        let mut jobs = self.jobs.write().await;
        let snap = self.observability.snapshot();
        *jobs = snap.jobs.iter().cloned().collect();
    }

    pub async fn complete_job(&self, id: &str) {
        self.observability.complete_job(id).await;

        {
            let mut jobs = self.jobs.write().await;
            let snap = self.observability.snapshot();
            *jobs = snap.jobs.iter().cloned().collect();
        }
        self.emit_event("job.completed", &serde_json::json!({"id": id}).to_string())
            .await;
    }

    pub async fn fail_job(&self, id: &str, error: &str) {
        self.observability.fail_job(id, error).await;

        {
            let mut jobs = self.jobs.write().await;
            let snap = self.observability.snapshot();
            *jobs = snap.jobs.iter().cloned().collect();
        }
        self.emit_event(
            "job.failed",
            &serde_json::json!({"id": id, "error": error}).to_string(),
        )
        .await;
    }

    // ── Config ──────────────────────────────────────────────────

    pub async fn vram_budget_for(&self, stone_name: &str, vram_total: u64) -> u64 {
        let config = self.config.read().await;
        config
            .stones
            .get(stone_name)
            .and_then(|s| s.vram_budget_mb)
            .map(|mb| mb * 1_048_576)
            .unwrap_or(vram_total)
    }

    // ── Recommendations (legacy — delegates to Intelligence) ────

    pub async fn refresh_recommendations(&self) {
        let models = self.directory_legacy.read().await.clone();
        let instances = self.instances.read().await.clone();
        let gpu_matrix = self.benchmark_run.read().await.gpu_matrix.clone();
        let pins = self.config.read().await.features.pins.clone();

        let mut cache = HashMap::with_capacity(15);
        for &cap in &[
            "quick", "chat", "synthesis", "vision", "ocr", "tools", "thinking",
            "embedding", "image", "video", "transcribe", "speech", "music",
            "rerank", "translate",
        ] {
            let pin = pins.get(cap).map(|s| s.as_str());
            let resp = crate::domain::recommendation::recommend(
                cap, &models, &instances, &gpu_matrix, pin,
            );
            if let Some(selected) = resp.selected {
                cache.insert(cap.to_string(), selected);
            }
        }

        *self.recommended_models.write().await = cache;
    }

    // ── Tending ─────────────────────────────────────────────────

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
