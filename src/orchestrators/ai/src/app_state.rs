//! `AppState` — the top-level state container shared by every HTTP
//! handler and background task.
//!
//! # ORCH-0030 R2 M3 shape
//!
//! After the trait switch, `AppState` no longer carries the legacy
//! `Directory` aggregate, the `RecommendationEngine`, or the `Skills`
//! aggregate — those are deleted. Routing is driven by the
//! [`crate::services::directory_subscriber::CapabilityDirectory`]
//! (built from capability events) and the
//! [`crate::services::provider_registry::ProviderRegistry`] (the
//! process-internal `name → Arc<dyn Provider>` map).

use std::path::PathBuf;
use std::sync::Arc;

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
use crate::services::skills::ProvisioningQueue;

#[derive(Clone)]
pub struct AppState {
    pub vocabularies: VocabularyRegistry,
    pub media_store: SharedMediaStore,
    pub job_store: Arc<dyn JobStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub dispatcher: Arc<Dispatcher>,
    pub catalog: Arc<CatalogBuilder>,
    /// Provisioning queue (ORCH-0029 Phase 2) — bounded-concurrency
    /// worker that downloads missing models into the local cache and
    /// pushes them to discovered ComfyUI instances via the Moss
    /// volume API.
    pub provisioning: Arc<ProvisioningQueue>,
    /// Absolute path to the orchestrator's data directory. Shared
    /// across handlers that need to write new files (import drafts,
    /// CRUD edits) into the on-disk layout.
    pub data_dir: PathBuf,
    /// Unified event bus (ORCH-0030 §1) — the orchestrator's single
    /// nervous system. Every domain publishes state transitions
    /// here; HTTP `/v1/events` exposes a glob-filtered view.
    pub events: Arc<EventBus>,
    /// Resources domain (ORCH-0030 §2) — physical stone resources
    /// (GPU VRAM, system memory) with claim-based accounting.
    pub resources: Arc<Resources>,
    /// Capability directory (ORCH-0030 §R2.2, §R2.8) — the read-only
    /// view of every provider's currently-declared capabilities and
    /// skills, rebuilt from bus events by the `DirectorySubscriber`.
    /// Authoritative source of routing decisions.
    pub capability_directory: Arc<CapabilityDirectory>,
    /// Provider registry (ORCH-0030 R2 M3) — process-internal
    /// `name → Arc<dyn Provider>` lookup. Populated at startup with
    /// every constructed adapter handle. The dispatcher reads from
    /// this to invoke `provider.onboard()`.
    pub provider_registry: Arc<ProviderRegistry>,
}
