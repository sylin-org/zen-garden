//! `Catalog` aggregate — DDD root of the Catalog bounded context.
//!
//! Ch3 of ARCH-0022 (Book V of ARCH-0017). Wraps a frozen
//! `Arc<ManifestRegistry>` (immutable after bootstrap) and a mutable
//! `RwLock<CatalogState>` (the compiled index) with typed
//! commands (`load`, `rebuild`), typed queries (`get_manifest`,
//! `get_compiled`, `compiled_snapshot`, `stats`, …), a
//! `CatalogChanged` internal event stream with two kinds (`Loaded`,
//! `Rebuilt`), `Arc<Metrics>` injection, and a `CatalogCache`
//! persistence port.
//!
//! ## Typed errors — first in the epic
//!
//! Commands return `Result<(), CatalogError>` rather than
//! `anyhow::Result`. Prior aggregates (Metrics, Tool, Topology, Jobs)
//! are either infallible or propagate `anyhow`. Catalog is the first
//! aggregate where mutations have structured failure modes worth
//! propagating: disk I/O, per-offering compile errors, fingerprint
//! hashing. See code-standards §10.
//!
//! ## Frozen input
//!
//! `ManifestRegistry` is a cross-crate type (used by rake,
//! orchestrators, peer stones). The aggregate holds it as an immutable
//! `Arc<ManifestRegistry>` — a frozen input rather than mutable state.
//! No internal lock, no mutation commands, no dirty flag.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{RwLock, broadcast};

use super::cache::CatalogCache;
use super::entry::CompiledOffering;
use super::error::CatalogError;
use super::event::{CatalogChanged, ChangeKind, LoadSource};
use super::fingerprint::{current_capabilities_hash, manifests_hash, moss_version_string};
use super::index::{OfferingsFingerprint, OfferingsIndex};
use super::state::CatalogState;
use crate::domain::Metrics;
use crate::domain::compatibility::compile_compatibility;
use garden_common::HardwareCapabilities;
use garden_common::manifests::{HwEntry, ManifestRegistry, Offering};

/// Capacity of the internal `CatalogChanged` broadcast channel.
///
/// Small — the catalog fires at most 2 events per process start
/// (one `Loaded`, one `Rebuilt`). 16 is more than enough headroom.
const CHANNEL_CAPACITY: usize = 16;

/// Summary stats returned by [`Catalog::stats`].
#[derive(Debug, Clone)]
pub struct CatalogStats {
    pub manifest_count: usize,
    pub compiled_count: usize,
    pub fingerprint: Option<OfferingsFingerprint>,
}

/// `Catalog` bounded context.
///
/// Persistent aggregate — the compiled index is cached to disk via the
/// injected `CatalogCache` port so subsequent cold starts can skip
/// manifest re-compilation when the fingerprint matches.
pub struct Catalog {
    /// Frozen source-of-truth manifest registry. Loaded in
    /// `bootstrap::build_state` before the aggregate is constructed.
    /// No internal lock — immutable after bootstrap.
    manifests: Arc<ManifestRegistry>,

    /// Compiled catalog snapshot. Starts `None`; populated by `load` or
    /// `rebuild`. Interior mutability via `RwLock` so commands can swap
    /// the snapshot without rebuilding the aggregate struct.
    state: RwLock<CatalogState>,

    /// Hardware capabilities snapshot source (shared with
    /// `current::Resources`). The aggregate does not own capabilities —
    /// it reads them via this handle at rebuild time.
    capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,

    /// Injected persistence port.
    cache: Arc<dyn CatalogCache>,

    /// Metrics aggregate.
    metrics: Arc<Metrics>,

    /// Internal domain event broadcast.
    changes: broadcast::Sender<CatalogChanged>,
}

impl Catalog {
    /// Registered domain name for Metrics.
    pub const NAME: &'static str = "catalog";

    /// Construct a new `Catalog` aggregate.
    ///
    /// The caller (`bootstrap::run`) passes the frozen
    /// `ManifestRegistry`, the shared capabilities handle, and the
    /// `CatalogCache` port. The compiled index starts empty (`None`);
    /// it is populated by the `load` or `rebuild` commands.
    pub async fn new(
        manifests: Arc<ManifestRegistry>,
        capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,
        cache: Arc<dyn CatalogCache>,
        metrics: Arc<Metrics>,
    ) -> Self {
        metrics
            .register_domain(Self::NAME, ChangeKind::ALL_NAMES)
            .await;
        let (changes, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            manifests,
            state: RwLock::new(CatalogState::empty()),
            capabilities,
            cache,
            metrics,
            changes,
        }
    }

    // ── Commands ────────────────────────────────────────────────────────

    /// Load the catalog from disk cache if fingerprint matches, else
    /// rebuild and persist. Idempotent: no-op if memory is already
    /// populated. Called by the `catalog-builder` task at startup.
    #[tracing::instrument(level = "debug", skip(self), fields(catalog.command = "load"))]
    pub async fn load(&self) -> Result<(), CatalogError> {
        let started = Instant::now();

        // Idempotent: already loaded → skip.
        {
            let guard = self.state.read().await;
            if guard.index.is_some() {
                return Ok(());
            }
        }

        // Snapshot current capabilities once.
        let caps = self.capabilities.read().await.clone();
        let caps_ref = caps.as_ref();

        // Try disk cache first.
        let disk_result = self
            .cache
            .load()
            .await
            .map_err(CatalogError::CacheReadFailed)?;

        if let Some(cached) = disk_result {
            let current_fp = self.build_fingerprint(caps_ref)?;
            if cached.fingerprint == current_fp {
                let compiled_count = cached.offerings.len();
                let fingerprint = cached.fingerprint.clone();
                *self.state.write().await = CatalogState {
                    index: Some(cached),
                };
                self.metrics
                    .record_mutation_latency(Self::NAME, started.elapsed())
                    .await;
                self.emit(CatalogChanged::Loaded {
                    compiled_count,
                    fingerprint,
                    source: LoadSource::DiskCache,
                })
                .await;
                return Ok(());
            }
        }

        // Cache miss or fingerprint mismatch → rebuild.
        let index = self.compile(caps_ref)?;
        self.cache
            .save(&index)
            .await
            .map_err(CatalogError::CacheWriteFailed)?;
        let compiled_count = index.offerings.len();
        let fingerprint = index.fingerprint.clone();
        *self.state.write().await = CatalogState { index: Some(index) };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.emit(CatalogChanged::Loaded {
            compiled_count,
            fingerprint,
            source: LoadSource::FreshRebuild,
        })
        .await;
        Ok(())
    }

    /// Force a rebuild from current manifests + current capabilities,
    /// bypassing disk cache for the read path. Persists the new index
    /// on success. Called by `hardware-detection` after capabilities
    /// become available.
    #[tracing::instrument(level = "debug", skip(self), fields(catalog.command = "rebuild"))]
    pub async fn rebuild(&self) -> Result<(), CatalogError> {
        let started = Instant::now();

        let caps = self.capabilities.read().await.clone();
        let caps_ref = caps.as_ref();

        let index = self.compile(caps_ref)?;

        // Check if fingerprint actually changed.
        let fingerprint_changed = {
            let guard = self.state.read().await;
            guard
                .index
                .as_ref()
                .map(|old| old.fingerprint != index.fingerprint)
                .unwrap_or(true)
        };

        self.cache
            .save(&index)
            .await
            .map_err(CatalogError::CacheWriteFailed)?;

        let compiled_count = index.offerings.len();
        let fingerprint = index.fingerprint.clone();
        *self.state.write().await = CatalogState { index: Some(index) };

        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        self.emit(CatalogChanged::Rebuilt {
            compiled_count,
            fingerprint,
            fingerprint_changed,
        })
        .await;
        Ok(())
    }

    // ── Queries ─────────────────────────────────────────────────────────

    /// Return the manifest entry for `name`, or `None` if unknown.
    /// Owned-value query (clones the `Offering`).
    pub fn get_manifest(&self, name: &str) -> Option<Offering> {
        self.manifests.sw.get(name).cloned()
    }

    /// Hardware manifest lookup.
    pub fn find_hw_manifest(
        &self,
        manufacturer: Option<&str>,
        product: Option<&str>,
    ) -> Option<HwEntry> {
        self.manifests
            .hw
            .find_matching(manufacturer, product)
            .cloned()
    }

    /// Total manifest count — useful for logging.
    pub fn manifest_count(&self) -> usize {
        self.manifests.sw.entries.len()
    }

    /// Compiled offering by name. Returns `None` for either "catalog
    /// not loaded" or "unknown offering". Clones the `CompiledOffering`.
    pub async fn get_compiled(&self, name: &str) -> Option<CompiledOffering> {
        let guard = self.state.read().await;
        guard
            .index
            .as_ref()
            .and_then(|idx| idx.offerings.iter().find(|o| o.name == name).cloned())
    }

    /// Full compiled snapshot. Clones the whole vector (at most ~100
    /// items).
    pub async fn compiled_snapshot(&self) -> Option<Vec<CompiledOffering>> {
        let guard = self.state.read().await;
        guard.index.as_ref().map(|idx| idx.offerings.clone())
    }

    /// Summary stats: manifest count, compiled count, fingerprint.
    pub async fn stats(&self) -> CatalogStats {
        let guard = self.state.read().await;
        CatalogStats {
            manifest_count: self.manifests.sw.entries.len(),
            compiled_count: guard
                .index
                .as_ref()
                .map(|idx| idx.offerings.len())
                .unwrap_or(0),
            fingerprint: guard.index.as_ref().map(|idx| idx.fingerprint.clone()),
        }
    }

    /// Whether the compiled snapshot has been loaded yet.
    pub async fn is_loaded(&self) -> bool {
        self.state.read().await.index.is_some()
    }

    /// Direct access to the frozen manifest registry. Used by infra
    /// callers that need the full registry (e.g., infrastructure
    /// handlers). Returns a cheap `Arc` clone.
    pub fn manifests(&self) -> &Arc<ManifestRegistry> {
        &self.manifests
    }

    // ── Events ──────────────────────────────────────────────────────────

    /// Subscribe to the internal `CatalogChanged` stream.
    pub fn changes(&self) -> broadcast::Receiver<CatalogChanged> {
        self.changes.subscribe()
    }

    // ── Internals ───────────────────────────────────────────────────────

    /// Build the fingerprint for the current state (moss version +
    /// capabilities hash + manifest content hash).
    fn build_fingerprint(
        &self,
        caps: Option<&HardwareCapabilities>,
    ) -> Result<OfferingsFingerprint, CatalogError> {
        Ok(OfferingsFingerprint {
            moss_version: moss_version_string(),
            capabilities_hash: current_capabilities_hash(caps),
            templates_hash: manifests_hash(&self.manifests)
                .map_err(CatalogError::ManifestHashFailed)?,
        })
    }

    /// Compile all manifests into a fresh `OfferingsIndex`.
    fn compile(&self, caps: Option<&HardwareCapabilities>) -> Result<OfferingsIndex, CatalogError> {
        let fingerprint = self.build_fingerprint(caps)?;

        let mut entries: Vec<_> = self.manifests.sw.entries.values().collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let mut offerings = Vec::with_capacity(entries.len());
        for entry in entries {
            let mut template =
                entry
                    .parse_template()
                    .map_err(|e| CatalogError::CompilationFailed {
                        offering: entry.name.clone(),
                        source: e,
                    })?;
            let compatibility = compile_compatibility(&mut template, caps);

            // Validate the ceremony policy at compile time (ORCH-0041): a
            // misconfigured quiesceable policy (missing quiesce/resume) is
            // downgraded to the safe pause-only default rather than silently
            // snapshotting a live database.
            let ceremony = entry.ceremony.clone().unwrap_or_default();
            let ceremony = match ceremony.validate() {
                Ok(()) => ceremony,
                Err(e) => {
                    tracing::warn!(
                        offering = %entry.name,
                        error = %e,
                        "invalid ceremony policy; using safe pause-only default"
                    );
                    garden_common::manifests::CeremonyPolicy::default()
                }
            };

            offerings.push(CompiledOffering {
                name: entry.name.clone(),
                category: entry.category.clone(),
                description: entry.description(),
                tags: entry.tags(),
                image: template.image,
                command: template.command,
                config_files: template.config_files,
                ports: template.ports,
                environment: template.environment,
                volumes: template.volumes,
                compatibility,
                tasks: template.tasks,
                network: template.network,
                coordination: entry.coordination.clone(),
                device_requests: template.device_requests,
                resource_limits: template.resource_limits,
                healthcheck: template.healthcheck,
                ceremony,
            });
        }

        Ok(OfferingsIndex {
            fingerprint,
            generated_at: chrono::Utc::now().to_rfc3339(),
            offerings,
        })
    }

    /// Record the domain event counter and broadcast the event.
    async fn emit(&self, event: CatalogChanged) {
        self.metrics
            .record_domain_event(Self::NAME, event.kind().name())
            .await;
        let _ = self.changes.send(event);
    }
}
