//! `AppState` — the top-level state container shared by every HTTP
//! handler and background task.

use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::directory::Directory;
use crate::domain::events::EventBus;
use crate::domain::idempotency::IdempotencyStore;
use crate::domain::jobs::JobStore;
use crate::domain::media::SharedMediaStore;
use crate::domain::resources::Resources;
use crate::domain::vocabulary::VocabularyRegistry;
use crate::services::catalog_builder::CatalogBuilder;
use crate::services::dispatcher::Dispatcher;
use crate::services::recommendation::RecommendationEngine;
use crate::services::skills::{ProvisioningQueue, Skills};

#[derive(Clone)]
pub struct AppState {
    pub directory: Arc<Directory>,
    pub vocabularies: VocabularyRegistry,
    pub media_store: SharedMediaStore,
    pub job_store: Arc<dyn JobStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub dispatcher: Arc<Dispatcher>,
    pub recommendation: Arc<RecommendationEngine>,
    pub catalog: Arc<CatalogBuilder>,
    /// Skills aggregate (ORCH-0029) — dynamic per-skill state
    /// (registration metadata, per-instance readiness, AI naming
    /// updates) parallel to the Directory's static schema.
    pub skills: Arc<Skills>,
    /// Provisioning queue (ORCH-0029 Phase 2) — bounded-concurrency
    /// worker that downloads missing models into the local cache
    /// and pushes them to discovered ComfyUI instances via the Moss
    /// volume API.
    pub provisioning: Arc<ProvisioningQueue>,
    /// Absolute path to the orchestrator's data directory.
    /// Shared across handlers that need to write new files
    /// (import drafts, CRUD edits) into the on-disk layout.
    pub data_dir: PathBuf,
    /// Unified event bus (ORCH-0030 §1) — the orchestrator's single
    /// nervous system. Every domain publishes state transitions here;
    /// HTTP `/v1/events` exposes a glob-filtered view.
    pub events: Arc<EventBus>,
    /// Resources domain (ORCH-0030 §2) — physical stone resources
    /// (GPU VRAM, system memory) with claim-based accounting.
    pub resources: Arc<Resources>,
}
