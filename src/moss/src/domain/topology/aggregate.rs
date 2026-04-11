//! `Topology` aggregate — DDD root of the Topology bounded context.
//!
//! Ch3 of ARCH-0020 (Book III of ARCH-0017). Wraps the existing
//! [`TopologyCache`] and [`TopologyDirtyFlag`] handles (from
//! [`super::mod`]) with a typed command/query surface, `Arc<Metrics>`
//! integration, `TopologyChanged` event stream, and two injected
//! ports: [`ChirpTransport`] and [`TopologyStore`].
//!
//! ## Strangler phase (Ch3–Ch5)
//!
//! The aggregate shares its cache + dirty-flag handles with the
//! existing `current::Topology { cache, dirty }` sub-struct on
//! AppState via `Arc` cloning. Both paths point at the same backing
//! `RwLock<HashMap<String, TopologyEntry>>` and the same
//! `AtomicBool`. The 42 existing `topology::free_fn(&cache, ...)`
//! caller sites continue to compile unchanged; the aggregate's typed
//! commands read and mutate the same storage via wrapper methods
//! that delegate to the existing free functions in `super::mod`.
//!
//! Ch5 migrates callers to typed commands, deletes the free
//! functions from `super::mod`, and marks the cache/dirty fields on
//! the aggregate private. Ch5 also deletes the `current::Topology`
//! sub-struct and promotes `state.topology` to a top-level AppState
//! field.

use super::event::{ChangeKind, TopologyChanged};
use super::store::TopologyStore;
use super::transport::ChirpTransport;
use super::{TopologyCache, TopologyDirtyFlag};
use crate::domain::Metrics;
use garden_common::{
    HardwareCapabilities, PeerAddress, StoneStatus, TopologyEntry, TopologyServiceEntry,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

/// Explicit inputs for `Topology::build_self_entry`.
///
/// The aggregate does not hold a back-reference to `AppState`. Callers
/// assemble these values from their context before invoking the
/// self-entry commands.
#[derive(Debug, Clone)]
pub struct SelfEntryInputs {
    pub stone_id: String,
    pub stone_name: String,
    pub address: PeerAddress,
    pub health: String,
    pub mac: Option<String>,
    pub capabilities: Option<HardwareCapabilities>,
    pub tags: Vec<String>,
    pub services: Vec<TopologyServiceEntry>,
    pub moss_version: String,
    pub network_ready: bool,
}

/// Default capacity for the internal `TopologyChanged` broadcast channel.
const CHANGES_CHANNEL_CAPACITY: usize = 512;

/// `Topology` bounded context.
///
/// Private state: the shared `TopologyCache` + `TopologyDirtyFlag`
/// handles (fields are `pub(crate)` during the strangler phase so
/// existing `state.current.topology.cache` sites still compile).
#[derive(Clone)]
pub struct Topology {
    /// Peer cache shared with the existing `current::Topology`
    /// sub-struct during the strangler phase. Ch5 flips to private.
    pub(crate) cache: TopologyCache,

    /// Persistence dirty flag shared during the strangler phase.
    pub(crate) dirty: TopologyDirtyFlag,

    /// Injected chirp transport port.
    chirp: Arc<dyn ChirpTransport>,

    /// Injected persistence port.
    #[allow(dead_code)] // Wired in Ch5 (save path)
    store: Arc<dyn TopologyStore>,

    /// Metrics aggregate.
    #[allow(dead_code)] // Wired in Ch3 events
    metrics: Arc<Metrics>,

    /// Internal domain event broadcast.
    changes: broadcast::Sender<TopologyChanged>,
}

impl Topology {
    /// Registered domain name for Metrics.
    pub const NAME: &'static str = "topology";

    /// Construct a new `Topology` aggregate.
    ///
    /// `cache` and `dirty` are passed in rather than created — during
    /// the strangler phase the aggregate shares handles with the
    /// existing `current::Topology` sub-struct. Ch5 flips to
    /// constructor-owned state loaded from the store.
    pub async fn new(
        cache: TopologyCache,
        dirty: TopologyDirtyFlag,
        chirp: Arc<dyn ChirpTransport>,
        store: Arc<dyn TopologyStore>,
        metrics: Arc<Metrics>,
    ) -> Self {
        metrics
            .register_domain(Self::NAME, ChangeKind::ALL_NAMES)
            .await;

        let (changes, _) = broadcast::channel(CHANGES_CHANNEL_CAPACITY);

        Self {
            cache,
            dirty,
            chirp,
            store,
            metrics,
            changes,
        }
    }

    /// Subscribe to the internal `TopologyChanged` domain event stream.
    pub fn changes(&self) -> broadcast::Receiver<TopologyChanged> {
        self.changes.subscribe()
    }

    /// Broadcast a chirp via the injected transport.
    ///
    /// Emits a `SelfEntryChirped` event and records mutation latency
    /// on success. Failures are propagated to the caller — most
    /// consumers log and continue.
    pub async fn chirp(&self, entry: &TopologyEntry) -> anyhow::Result<()> {
        let started = Instant::now();
        let result = self.chirp.chirp(entry).await;
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        if result.is_ok() {
            let event = TopologyChanged::SelfEntryChirped {
                stone_id: entry.stone_id.clone(),
                stone_name: entry.stone_name.clone(),
            };
            self.emit(event).await;
        }
        result
    }

    // ── Queries ─────────────────────────────────────────────────────────

    /// All stones (online + offline) in the cache.
    pub async fn all_stones(&self) -> Vec<TopologyEntry> {
        super::get_all_stones(&self.cache).await
    }

    /// Online stones only.
    pub async fn online_stones(&self) -> Vec<TopologyEntry> {
        super::get_online_stones(&self.cache).await
    }

    /// Look up a stone by id.
    pub async fn get_by_id(&self, stone_id: &str) -> Option<TopologyEntry> {
        super::get_stone_by_id(&self.cache, stone_id).await
    }

    /// Look up a stone by name.
    pub async fn get_by_name(&self, stone_name: &str) -> Option<TopologyEntry> {
        super::get_stone_by_name(&self.cache, stone_name).await
    }

    /// Total stones (online + offline).
    pub async fn count(&self) -> usize {
        super::count_stones(&self.cache).await
    }

    /// Online stones only.
    pub async fn online_count(&self) -> usize {
        super::count_online_stones(&self.cache).await
    }

    /// Whether the cache has unsaved mutations.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Assemble a `TopologyEntry` from the given `SelfEntryInputs`.
    ///
    /// Pure function — no state mutation. Callers typically chain
    /// this with `chirp(&entry)` to broadcast, or with `store.save`
    /// for persistence.
    pub fn build_self_entry(&self, inputs: SelfEntryInputs) -> TopologyEntry {
        let now = chrono::Utc::now();
        TopologyEntry {
            stone_id: inputs.stone_id,
            stone_name: inputs.stone_name,
            address: inputs.address,
            moss_version: inputs.moss_version,
            mac: inputs.mac,
            health: inputs.health,
            capabilities: inputs.capabilities,
            services: inputs.services,
            status: StoneStatus::Online,
            discovered_at: now,
            last_seen: now,
            tags: inputs.tags,
            gateways: vec![],
        }
    }

    /// Service-change self-entry refresh: build entry and optionally
    /// chirp (skipping if network is not ready).
    ///
    /// Called after any offerings mutation. The `auto_chirp` flag
    /// allows callers to suppress the chirp when they'll be following
    /// up with a batch of changes.
    pub async fn sync_services(
        &self,
        inputs: SelfEntryInputs,
        auto_chirp: bool,
    ) -> anyhow::Result<()> {
        if !auto_chirp || !inputs.network_ready {
            return Ok(());
        }
        let entry = self.build_self_entry(inputs);
        self.chirp(&entry).await
    }

    /// Capability-change self-entry refresh.
    pub async fn sync_capabilities(
        &self,
        inputs: SelfEntryInputs,
        auto_chirp: bool,
    ) -> anyhow::Result<()> {
        if !auto_chirp || !inputs.network_ready {
            return Ok(());
        }
        let entry = self.build_self_entry(inputs);
        self.chirp(&entry).await
    }

    /// Health-transition self-entry refresh.
    pub async fn update_stone_health(
        &self,
        inputs: SelfEntryInputs,
        auto_chirp: bool,
    ) -> anyhow::Result<()> {
        if !auto_chirp || !inputs.network_ready {
            return Ok(());
        }
        let entry = self.build_self_entry(inputs);
        self.chirp(&entry).await
    }

    /// Resolution-change self-entry refresh. The caller is responsible
    /// for updating `current.address` / `current.mac` and for the
    /// mDNS re-registration (Book X's Discovery aggregate will absorb
    /// mDNS ownership). This method only takes the updated values
    /// via `SelfEntryInputs` and chirps.
    pub async fn announce_resolution_change(&self, inputs: SelfEntryInputs) -> anyhow::Result<()> {
        let entry = self.build_self_entry(inputs);
        self.chirp(&entry).await
    }

    // ── Commands ────────────────────────────────────────────────────────
    //
    // Ch3 wraps the existing free-function commands in `super::mod`
    // behind typed methods that record metrics. The event emission
    // for interesting transitions is a Ch5 concern — Ch3 only covers
    // the `StoneForgotten` and `StoneDiscovered` cases via the simple
    // lookup-then-call pattern below. Ch5 adds status-transition
    // detection by comparing pre/post snapshots inside
    // `upsert_from_chirp` and `maintain`.

    /// Forget a stone by name (operator action).
    pub async fn forget_stone(&self, stone_name: &str) -> Option<TopologyChanged> {
        let started = Instant::now();
        let removed = super::forget_stone_dirty(&self.cache, stone_name, &self.dirty).await;
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        if removed {
            let event = TopologyChanged::StoneForgotten {
                stone_name: stone_name.to_string(),
            };
            self.emit(event.clone()).await;
            Some(event)
        } else {
            None
        }
    }

    /// Mark a stone offline by id (goodbye / explicit offline path).
    pub async fn mark_stone_offline(&self, stone_id: &str) -> Option<TopologyChanged> {
        let pre = super::get_stone_by_id(&self.cache, stone_id).await;
        let started = Instant::now();
        let changed = super::mark_stone_offline_dirty(&self.cache, stone_id, &self.dirty).await;
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;
        if changed {
            // Only fire the transition event if the stone was previously online.
            if let Some(pre) = pre
                && pre.status == garden_common::StoneStatus::Online
            {
                let event = TopologyChanged::StoneOffline {
                    stone_id: stone_id.to_string(),
                    stone_name: pre.stone_name,
                };
                self.emit(event.clone()).await;
                return Some(event);
            }
        }
        None
    }

    // ── Internals ───────────────────────────────────────────────────────

    async fn emit(&self, event: TopologyChanged) {
        self.metrics
            .record_domain_event(Self::NAME, event.kind().name())
            .await;
        let _ = self.changes.send(event);
    }
}
