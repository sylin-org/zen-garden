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
use crate::services::directory_subscriber::CapabilityDirectory;
use crate::services::dispatcher::Dispatcher;
use crate::services::provider_registry::ProviderRegistry;
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
    /// Capability directory (ORCH-0030 §R2.2, §R2.8) — read-only
    /// view of every provider's currently-declared capabilities and
    /// skills, rebuilt from bus events by the DirectorySubscriber.
    /// Empty until adapters start publishing capability events
    /// (commits 7+).
    pub capability_directory: Arc<CapabilityDirectory>,
    /// Provider registry (ORCH-0030 R2 M1) — process-internal
    /// `name → Arc<dyn Provider>` lookup. Populated at startup with
    /// every constructed adapter handle. **Nothing reads from this
    /// in M1**; the dispatcher continues to look up providers via
    /// the legacy `Directory` aggregate. The atomic switchover
    /// happens in M3 (the trait switch milestone).
    pub provider_registry: Arc<ProviderRegistry>,
}
