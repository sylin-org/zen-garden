//! `Tool` aggregate — the DDD root of the Tool bounded context.
//!
//! Ch3 of ARCH-0019 (Book II of ARCH-0017). Wraps the
//! [`GardenRegistryInner`](super::registry::GardenRegistryInner) state
//! in a typed command/query surface with:
//!
//! - `Arc<Metrics>` integration (mutation latency + event counters)
//! - `broadcast::Sender<ToolChanged>` internal event stream
//! - `broadcast::Sender<ToolDelta>` wire-format stream (preserved)
//! - Typed commands that own the write path
//! - Typed queries that return owned values
//!
//! ## Strangler phase (Ch3–Ch5)
//!
//! The aggregate's state is held in a `pub(crate)` field `registry`
//! during the strangler phase. Fifty existing call sites continue to
//! compile against the field directly (`state.tool.registry.read().await`)
//! while the new typed API grows alongside. Ch6 migrates all call
//! sites to typed methods and marks the field private. The field-level
//! strangler is documented as a Ch3 refinement of ARCH-0019's original
//! Offerings strangler plan — same end state, fewer moving parts.
//!
//! ## Pattern deviations (see ARCH-0019 §"Pattern deviations")
//!
//! 1. **No Store port** — the registry is ephemeral, rebuilt from
//!    offerings + storage + remote beacons + TTL. No persistence.
//!
//! 2. **Dual event streams** — `changes()` carries the internal
//!    [`ToolChanged`] domain type; `delta_stream()` carries the
//!    wire-format [`ToolDelta`] consumed by SSE and UDP beacon
//!    subscribers. Both are fed atomically from every command.
//!
//! 3. **Queries return owned values** — no `&RegistryEntry` across the
//!    lock boundary. Hot-path callers use dedicated typed queries that
//!    return already-filtered results (`storage_primary`,
//!    `find_s3_gateways`, `route_to_primary`) rather than iterating.

use super::event::{ChangeKind, ToolChanged};
use super::registry::{EntryOrigin, GardenRegistry, RegistryEntry, ToolQuery, new_registry};
use super::transport::ToolsBeaconTransport;
use crate::domain::Metrics;
use garden_common::tools::{GardenTool, ToolDelta, ToolsBeacon};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Default capacity for the internal `ToolChanged` broadcast channel.
///
/// Sized generously — tool changes fan out to multiple projection
/// tasks (Metrics, Topology in Book III, future subscribers) and lag
/// tolerance is preferred over backpressure.
const CHANGES_CHANNEL_CAPACITY: usize = 1024;

/// Garden-wide tool aggregate.
///
/// Owns the registry of all `GardenTool` entries — local (projected
/// from offerings and storage), Gateway (orchestrator-registered with
/// TTL), and Announced (received from remote stones via beacon).
///
/// Construction registers the `tool` domain with `Metrics` using the
/// register-with-kinds pattern — the hot path never mutates the kind
/// map, so per-command event recording is lock-free on the kind
/// dimension.
#[derive(Clone)]
pub struct Tool {
    /// Registry state.
    ///
    /// `pub(crate)` during the strangler phase (Ch3–Ch5). Fifty legacy
    /// call sites access this directly; Ch6 migrates them to typed
    /// methods on [`Tool`] and flips this field to private.
    pub(crate) registry: GardenRegistry,

    /// Wire-format delta broadcast — preserved contract for SSE and
    /// UDP beacon subscribers. Fed from every command.
    pub(crate) delta: broadcast::Sender<ToolDelta>,

    /// Internal domain event broadcast. Fed from every command
    /// alongside `delta`. Consumed by metrics, projection tasks, and
    /// in-process subscribers. See [`Tool::changes`].
    changes: broadcast::Sender<ToolChanged>,

    /// Metrics aggregate for mutation latency + per-kind event counts.
    metrics: Arc<Metrics>,

    /// UDP beacon transport port. Injected at construction; the
    /// aggregate publishes tool deltas to remote stones via this
    /// instead of calling `crate::infra::*` directly.
    transport: Arc<dyn ToolsBeaconTransport>,
}

impl Tool {
    /// Registered domain name for Metrics.
    pub const NAME: &'static str = "tool";

    /// Construct a new `Tool` aggregate and register it with Metrics.
    ///
    /// Register-with-kinds pattern: the kind set is known at
    /// construction time and never changes, so the Metrics hot path
    /// for this domain never takes a write lock on the kind map.
    pub async fn new(
        metrics: Arc<Metrics>,
        delta: broadcast::Sender<ToolDelta>,
        transport: Arc<dyn ToolsBeaconTransport>,
    ) -> Self {
        metrics
            .register_domain(Self::NAME, ChangeKind::ALL_NAMES)
            .await;

        let (changes, _) = broadcast::channel(CHANGES_CHANNEL_CAPACITY);

        Self {
            registry: new_registry(),
            delta,
            changes,
            metrics,
            transport,
        }
    }

    // ── Transport passthroughs ──────────────────────────────────────────

    /// Publish an incremental tools beacon (skips empty beacons).
    ///
    /// Called from the projection task after commands produce deltas.
    /// Stone identity + endpoint are passed in at call time — the
    /// aggregate does not own them.
    pub async fn publish_incremental(
        &self,
        stone_id: &str,
        stone_name: &str,
        endpoint: &str,
        deltas: Vec<ToolDelta>,
    ) -> anyhow::Result<()> {
        self.transport
            .broadcast_incremental(stone_id, stone_name, endpoint, deltas)
            .await
    }

    /// Publish a snapshot tools beacon (authoritative full set).
    ///
    /// Used by announcer / discovery-join paths to publish the
    /// stone's full local projection on startup and periodically
    /// afterwards, so late-joining peers can fill their registries.
    pub async fn publish_snapshot(
        &self,
        stone_id: &str,
        stone_name: &str,
        endpoint: &str,
        deltas: Vec<ToolDelta>,
    ) -> anyhow::Result<()> {
        self.transport
            .broadcast_snapshot(stone_id, stone_name, endpoint, deltas)
            .await
    }

    // ── Event subscriptions ─────────────────────────────────────────────

    /// Subscribe to the internal `ToolChanged` domain event stream.
    ///
    /// Name the local receiver by the consumer's purpose:
    ///
    /// ```rust,ignore
    /// let tool_projection_feed = state.tool.changes();
    /// ```
    pub fn changes(&self) -> broadcast::Receiver<ToolChanged> {
        self.changes.subscribe()
    }

    /// Subscribe to the wire-format `ToolDelta` stream.
    ///
    /// This is the existing SSE / UDP beacon contract. Kept alongside
    /// [`Tool::changes`] as a documented pattern deviation — see
    /// ARCH-0019 §"Pattern deviations".
    pub fn delta_stream(&self) -> broadcast::Receiver<ToolDelta> {
        self.delta.subscribe()
    }

    // ── Queries ─────────────────────────────────────────────────────────

    /// Filtered snapshot of all registry entries.
    ///
    /// Returns `(cursor, sorted_tools)` — the cursor lets SSE clients
    /// resume from the exact point the snapshot was taken without
    /// missing or duplicating events.
    pub async fn snapshot(&self, query: &ToolQuery) -> (u64, Vec<GardenTool>) {
        self.registry.read().await.snapshot(query)
    }

    /// Replay deltas since a cursor, filtered by query.
    ///
    /// Used by SSE reconnect and by consumers that need to catch up
    /// after a lag.
    pub async fn deltas_since(&self, since_cursor: u64, query: &ToolQuery) -> Vec<ToolDelta> {
        self.registry.read().await.deltas_since(since_cursor, query)
    }

    /// Fetch a tool by registry key.
    pub async fn get(&self, key: &str) -> Option<GardenTool> {
        self.registry.read().await.get_tool(key).cloned()
    }

    /// Current registry cursor.
    pub async fn current_cursor(&self) -> u64 {
        self.registry.read().await.current_cursor()
    }

    /// Look up the cursor at which a specific event was recorded, if
    /// the event is still in history.
    pub async fn cursor_for_event_id(&self, event_id: &str) -> Option<u64> {
        self.registry.read().await.cursor_for_event_id(event_id)
    }

    /// All storage entries across all stones (owned clones).
    pub async fn storage_entries(&self) -> Vec<RegistryEntry> {
        self.registry
            .read()
            .await
            .storage_entries()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Storage entries for a specific seed bank name (owned clones).
    pub async fn storage_by_name(&self, name: &str) -> Vec<RegistryEntry> {
        self.registry
            .read()
            .await
            .storage_by_name(name)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Primary replica for a named seed bank (owned clone).
    pub async fn storage_primary(&self, name: &str) -> Option<RegistryEntry> {
        self.registry.read().await.storage_primary(name).cloned()
    }

    /// Route to a Primary replica excluding the given stone.
    ///
    /// Returns `(stone_id, endpoint, seed_bank_id)` for the target.
    pub async fn route_to_primary(
        &self,
        name: &str,
        exclude_stone: &str,
    ) -> Option<(String, String, String)> {
        self.registry
            .read()
            .await
            .route_to_primary(name, exclude_stone)
    }

    /// All storage entries advertising the S3 protocol (owned clones).
    pub async fn find_s3_gateways(&self) -> Vec<RegistryEntry> {
        self.registry
            .read()
            .await
            .find_s3_gateways()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Storage entry by seed bank id (owned clone).
    pub async fn storage_by_id(&self, id: &str) -> Option<RegistryEntry> {
        self.registry.read().await.storage_by_id(id).cloned()
    }

    /// Storage entries grouped by stone id (owned clones).
    pub async fn storage_grouped_by_stone(&self) -> BTreeMap<String, Vec<RegistryEntry>> {
        self.registry
            .read()
            .await
            .storage_grouped_by_stone()
            .into_iter()
            .map(|(stone_id, entries)| (stone_id, entries.into_iter().cloned().collect()))
            .collect()
    }

    /// Number of stones with storage entries.
    pub async fn storage_stone_count(&self) -> usize {
        self.registry.read().await.storage_stone_count()
    }

    /// Total number of storage entries across all stones.
    pub async fn storage_count(&self) -> usize {
        self.registry.read().await.storage_count()
    }

    /// Moss endpoint for a given stone id.
    pub async fn stone_endpoint(&self, stone_id: &str) -> Option<String> {
        self.registry
            .read()
            .await
            .stone_endpoint(stone_id)
            .map(String::from)
    }

    /// True if any gateway entry handles the given offering type.
    pub async fn handles_offering(&self, offering: &str) -> bool {
        self.registry.read().await.handles_offering(offering)
    }

    /// Set of offering types that have registered gateway handlers.
    pub async fn handled_offerings(&self) -> HashSet<String> {
        self.registry.read().await.handled_offerings()
    }

    /// Build a wire-format snapshot of this stone's local (`Local`-origin)
    /// entries for a `ToolsBeacon` broadcast. Used by the periodic
    /// announcer and the discovery join path to publish authoritative
    /// tool state to remote peers.
    pub async fn local_snapshot_for_beacon(&self, stone_id: &str) -> Vec<ToolDelta> {
        self.registry
            .read()
            .await
            .local_snapshot_for_beacon(stone_id)
    }

    // ── Commands ────────────────────────────────────────────────────────

    /// Upsert a single entry with an optional TTL.
    ///
    /// Primary write path for gateway registration and local projection.
    /// Returns `Some(ToolChanged::Upserted)` if the entry changed,
    /// `None` if it was a no-op (content unchanged, TTL refresh only).
    pub async fn upsert(
        &self,
        tool: GardenTool,
        origin: EntryOrigin,
        expires_at: Option<Instant>,
    ) -> Option<ToolChanged> {
        let started = Instant::now();
        let delta = {
            let mut reg = self.registry.write().await;
            reg.upsert_with_expiry(tool, origin.clone(), expires_at)
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        let delta = delta?;
        let cursor = delta.cursor;
        let _ = self.delta.send(delta.clone());
        let event = ToolChanged::Upserted {
            delta,
            origin,
            cursor,
        };
        self.emit_change(event.clone()).await;
        Some(event)
    }

    /// Register (or refresh) a gateway entry with a TTL.
    ///
    /// Convenience wrapper around `upsert` for the gateway API.
    pub async fn register_gateway(&self, tool: GardenTool, ttl: Duration) -> Option<ToolChanged> {
        let expires_at = Instant::now() + ttl;
        self.upsert(tool, EntryOrigin::Gateway, Some(expires_at))
            .await
    }

    /// Deregister a gateway entry for a given offering on a given stone.
    pub async fn deregister_gateway(&self, offering: &str, stone_id: &str) -> Option<ToolChanged> {
        let started = Instant::now();
        let delta = {
            let mut reg = self.registry.write().await;
            reg.remove_gateway(offering, stone_id)
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        let delta = delta?;
        let cursor = delta.cursor;
        let _ = self.delta.send(delta.clone());
        let event = ToolChanged::Removed { delta, cursor };
        self.emit_change(event.clone()).await;
        Some(event)
    }

    /// Reap expired gateway entries.
    ///
    /// Called by the periodic registry-maintenance task. Emits one
    /// `ToolChanged::Reaped` event for the batch, plus one
    /// `ToolDelta::Remove` on the wire stream per reaped entry.
    pub async fn reap_expired_gateways(&self) -> Vec<ToolChanged> {
        let started = Instant::now();
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.reap_expired_gateways()
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        if deltas.is_empty() {
            return Vec::new();
        }

        let count = deltas.len();
        let mut events = Vec::with_capacity(count);
        let mut max_cursor = 0u64;
        for delta in &deltas {
            max_cursor = max_cursor.max(delta.cursor);
            let _ = self.delta.send(delta.clone());
            events.push(ToolChanged::Removed {
                delta: delta.clone(),
                cursor: delta.cursor,
            });
        }

        let batch = ToolChanged::Reaped {
            count,
            cursor: max_cursor,
        };
        self.emit_change(batch.clone()).await;
        events.push(batch);
        events
    }

    /// Reconcile a batch of local projections against the registry.
    ///
    /// Called by the local-projection path (offerings + storage →
    /// GardenTool). Removes stale local entries for this stone that
    /// aren't in the incoming batch. Returns one event per delta plus
    /// no batch-level event — local reconciliation is always driven
    /// by a specific state change.
    pub async fn reconcile_local(
        &self,
        local_stone_id: &str,
        incoming: Vec<GardenTool>,
    ) -> Vec<ToolChanged> {
        let started = Instant::now();
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.reconcile_local(local_stone_id, incoming, EntryOrigin::Local)
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        let mut events = Vec::with_capacity(deltas.len());
        for delta in deltas {
            let _ = self.delta.send(delta.clone());
            let cursor = delta.cursor;
            let event = match delta.kind {
                garden_common::tools::ToolDeltaKind::Upsert => ToolChanged::Upserted {
                    delta,
                    origin: EntryOrigin::Local,
                    cursor,
                },
                garden_common::tools::ToolDeltaKind::Remove => {
                    ToolChanged::Removed { delta, cursor }
                }
            };
            self.emit_change(event.clone()).await;
            events.push(event);
        }
        events
    }

    /// Apply a remote beacon received from a peer stone.
    ///
    /// Ingests upserts and removes from the beacon into the registry.
    /// Emits one `ToolChanged::BeaconApplied` batch event plus
    /// individual wire deltas for every affected entry.
    pub async fn apply_remote_beacon(&self, beacon: &ToolsBeacon) -> Vec<ToolChanged> {
        let started = Instant::now();
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.apply_remote_beacon(beacon)
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        if deltas.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::with_capacity(deltas.len() + 1);
        let mut max_cursor = 0u64;
        for delta in &deltas {
            max_cursor = max_cursor.max(delta.cursor);
            let _ = self.delta.send(delta.clone());
            let cursor = delta.cursor;
            let event = match delta.kind {
                garden_common::tools::ToolDeltaKind::Upsert => ToolChanged::Upserted {
                    delta: delta.clone(),
                    origin: EntryOrigin::Announced {
                        stone_id: beacon.stone_id.clone(),
                    },
                    cursor,
                },
                garden_common::tools::ToolDeltaKind::Remove => ToolChanged::Removed {
                    delta: delta.clone(),
                    cursor,
                },
            };
            events.push(event);
        }

        let batch = ToolChanged::BeaconApplied {
            stone_id: beacon.stone_id.clone(),
            delta_count: deltas.len(),
            cursor: max_cursor,
        };
        self.emit_change(batch.clone()).await;
        events.push(batch);
        events
    }

    /// Remove all registry entries for a departed stone.
    ///
    /// Called when a `STONE_GOODBYE` announcement is received or when
    /// a peer is marked offline. Emits one
    /// `ToolChanged::StoneRemoved` batch event plus individual wire
    /// deltas per removed entry.
    pub async fn remove_stone(&self, stone_id: &str) -> Vec<ToolChanged> {
        let started = Instant::now();
        let deltas = {
            let mut reg = self.registry.write().await;
            reg.remove_stone(stone_id)
        };
        self.metrics
            .record_mutation_latency(Self::NAME, started.elapsed())
            .await;

        if deltas.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::with_capacity(deltas.len() + 1);
        let mut max_cursor = 0u64;
        for delta in &deltas {
            max_cursor = max_cursor.max(delta.cursor);
            let _ = self.delta.send(delta.clone());
            events.push(ToolChanged::Removed {
                delta: delta.clone(),
                cursor: delta.cursor,
            });
        }

        let batch = ToolChanged::StoneRemoved {
            stone_id: stone_id.to_string(),
            delta_count: deltas.len(),
            cursor: max_cursor,
        };
        self.emit_change(batch.clone()).await;
        events.push(batch);
        events
    }

    // ── Internals ───────────────────────────────────────────────────────

    /// Emit a `ToolChanged` event on the internal stream and record
    /// the per-kind counter in Metrics.
    async fn emit_change(&self, event: ToolChanged) {
        self.metrics
            .record_domain_event(Self::NAME, event.kind().name())
            .await;
        let _ = self.changes.send(event);
    }
}
