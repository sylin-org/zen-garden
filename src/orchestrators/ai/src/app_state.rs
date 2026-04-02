//! Shared application state — thin facade over domain objects (ORCH-0020).
//!
//! Domains own their mutable state privately and publish immutable snapshots
//! via `tokio::sync::watch`. API handlers read snapshots with zero locks.

use crate::catalog::ProviderRegistry;
use crate::domain::directory_domain::DirectoryDomain;
use crate::domain::fitness::BenchmarkRun;
use crate::domain::intelligence::IntelligenceDomain;
use crate::domain::observability::ObservabilityDomain;
use crate::domain::registry::RegistryDomain;
use crate::domain::skills_domain::SkillsDomain;
use crate::domain::types::*;
use crate::offerings::cloud::CloudProviderStore;
use crate::offerings::ollama::OllamaClient;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tokio_util::sync::CancellationToken;

pub use orchestrator_common::events::DashboardEvent;
pub use orchestrator_common::persistence::TendedStone;

use crate::domain::directory_domain::DirectorySnapshot;
use crate::domain::intelligence::IntelligenceSnapshot;
use crate::domain::observability::ObservabilitySnapshot;
use crate::domain::registry::RegistrySnapshot;
use crate::domain::skills_domain::SkillsSnapshot;

/// Shared state for the AI Orchestrator.
///
/// Five domain objects own mutable state privately and publish snapshots.
/// API handlers call `domain.snapshot()` — zero locks, zero contention.
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

    // ── Secrets ──
    pub secrets: crate::infra::secrets::SecretsStore,

    // ── Channels (already lock-free) ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,
    pub metrics_tx: mpsc::UnboundedSender<MetricEvent>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,
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
        secrets: crate::infra::secrets::SecretsStore,
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
        let intelligence = Arc::new(IntelligenceDomain::new(intel_tx.clone()));
        let observability = Arc::new(ObservabilityDomain::new(obs_tx, metrics_enabled));
        let skills = Arc::new(SkillsDomain::new(skills_tx));

        Self {
            registry,
            directory,
            intelligence,
            observability,
            skills,
            providers: Arc::new(providers),
            ollama_client,
            koi_endpoint,
            explicit_stone,
            dashboard_port,
            data_dir,
            start_time: Instant::now(),
            config: Arc::new(RwLock::new(config)),
            cloud_store: Arc::new(RwLock::new(cloud_store)),
            tended_stone: Arc::new(RwLock::new(None)),
            benchmark_run: Arc::new(RwLock::new(BenchmarkRun::idle())),
            benchmark_cancel: Arc::new(RwLock::new(None)),
            secrets,
            dashboard_tx,
            metrics_tx,
            shutdown,
        }
    }

    // ── Events ──────────────────────────────────────────────────

    pub async fn emit_event(&self, event_type: &str, data: &str) {
        let _ = self.dashboard_tx.send(DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
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
