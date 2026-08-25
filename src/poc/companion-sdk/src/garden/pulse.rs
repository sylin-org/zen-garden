//! Pulse — the orchestrator.
//!
//! The single fan-in point every event passes through before reaching any
//! subscriber. Owns:
//!
//! - **Deduplication** — bounded FIFO cache keyed by [`EventId`]; re-ingesting
//!   an event with a seen id is silently dropped.
//! - **Validation** — syntactic kind format ([`is_valid_kind`]), namespace
//!   registration, and kind/payload coherence (`event.kind` must match
//!   `event.payload.kind()`).
//! - **Coalescing** — events whose payload declared `COALESCING = true` are
//!   buffered per-kind in a latest-wins map. [`Pulse::flush_coalesced`]
//!   drains the buffer into subscribers; Book VII's Companion wires a
//!   periodic timer to do this.
//! - **Fan-out** — validated, non-coalescing events are sent to a
//!   `tokio::sync::broadcast::Sender<Event>`. Subscribers attach via
//!   [`Pulse::subscribe`].
//! - **Metrics** — atomic counters for every outcome (accepted / deduped /
//!   coalescing / rejected variants / dropped-on-fanout / coalesced-flushed).
//!
//! See [COMPANION-0003] for the book ADR.
//!
//! # Example
//!
//! ```
//! use std::any::Any;
//! use garden_companion_sdk::garden::{Event, EventPayload, Pulse, IngestResult};
//!
//! #[derive(Debug)]
//! struct Tended;
//! impl EventPayload for Tended {
//!     const KIND: &'static str = "core.stone.tended";
//!     fn as_any(&self) -> &dyn Any { self }
//! }
//!
//! let pulse = Pulse::with_defaults();
//! pulse.register_namespace("core");
//!
//! let mut rx = pulse.subscribe();
//! let result = pulse.ingest(Event::new(Tended));
//! assert!(matches!(result, IngestResult::Accepted { subscribers: 1 }));
//!
//! // Receiver sees the event (use an async runtime in real code).
//! let delivered = rx.try_recv().unwrap();
//! assert_eq!(delivered.kind, "core.stone.tended");
//! ```
//!
//! [`EventId`]: super::EventId
//! [`is_valid_kind`]: super::is_valid_kind
//! [COMPANION-0003]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0003-pulse.md

use super::event::{Event, EventId, is_valid_kind, kind_namespace};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Default dedup cache capacity. ~4K ids at 16 bytes each = ~64KB, which
/// covers several minutes of events at typical garden rates.
const DEFAULT_DEDUP_CAPACITY: usize = 4096;

/// Default broadcast channel capacity. Subscribers that fall behind by
/// more than this many events see `RecvError::Lagged` and must recover.
const DEFAULT_BROADCAST_CAPACITY: usize = 1024;

/// Configuration for [`Pulse::new`]. Use [`Pulse::with_defaults`] unless
/// you specifically need to tune one of these.
#[derive(Debug, Clone, Copy)]
pub struct PulseConfig {
    /// Maximum number of event ids kept for deduplication. Oldest ids are
    /// evicted FIFO when new ones arrive beyond this count.
    pub dedup_capacity: usize,

    /// Broadcast channel capacity. Slow subscribers lose the oldest queued
    /// events past this bound with `RecvError::Lagged`.
    pub broadcast_capacity: usize,
}

impl Default for PulseConfig {
    fn default() -> Self {
        Self {
            dedup_capacity: DEFAULT_DEDUP_CAPACITY,
            broadcast_capacity: DEFAULT_BROADCAST_CAPACITY,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Outcome of [`Pulse::ingest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestResult {
    /// The event passed validation, was not a duplicate, and was fanned out
    /// to subscribers. `subscribers` is the count that received it.
    ///
    /// `subscribers: 0` means no receiver was attached at the time; the
    /// `dropped_on_fanout` metric also incremented.
    Accepted { subscribers: usize },

    /// The event is coalescing (`EventPayload::COALESCING == true`) and
    /// was buffered. Actual delivery happens on the next
    /// [`Pulse::flush_coalesced`] call.
    Coalescing,

    /// The event's id matched an entry in the dedup cache. The event was
    /// silently dropped; the caller may treat this as success.
    Duplicate,

    /// The event failed validation. See [`RejectReason`] for the specific
    /// check that failed.
    Rejected(RejectReason),
}

/// Why an event was rejected at ingest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// `event.kind` does not conform to the kind namespace grammar (see
    /// [`is_valid_kind`]).
    ///
    /// [`is_valid_kind`]: super::is_valid_kind
    InvalidKindFormat,

    /// The namespace prefix of `event.kind` has not been registered on
    /// this [`Pulse`] via [`Pulse::register_namespace`].
    UnregisteredNamespace,

    /// `event.kind` does not match the value returned by
    /// `event.payload.kind()` — the envelope and its payload disagree about
    /// what kind they are. Indicates the envelope was constructed with
    /// mismatched fields.
    KindPayloadMismatch,
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Atomic counters maintained by [`Pulse`]. Read via
/// [`Pulse::metrics`] as a [`PulseMetricsSnapshot`].
#[derive(Default, Debug)]
struct PulseMetrics {
    ingested: AtomicU64,
    accepted: AtomicU64,
    deduped: AtomicU64,
    coalescing: AtomicU64,
    coalesced_flushed: AtomicU64,
    rejected_invalid_kind: AtomicU64,
    rejected_unregistered_namespace: AtomicU64,
    rejected_kind_payload_mismatch: AtomicU64,
    dropped_on_fanout: AtomicU64,
}

impl PulseMetrics {
    fn snapshot(&self) -> PulseMetricsSnapshot {
        PulseMetricsSnapshot {
            ingested: self.ingested.load(Ordering::Relaxed),
            accepted: self.accepted.load(Ordering::Relaxed),
            deduped: self.deduped.load(Ordering::Relaxed),
            coalescing: self.coalescing.load(Ordering::Relaxed),
            coalesced_flushed: self.coalesced_flushed.load(Ordering::Relaxed),
            rejected_invalid_kind: self.rejected_invalid_kind.load(Ordering::Relaxed),
            rejected_unregistered_namespace: self
                .rejected_unregistered_namespace
                .load(Ordering::Relaxed),
            rejected_kind_payload_mismatch: self
                .rejected_kind_payload_mismatch
                .load(Ordering::Relaxed),
            dropped_on_fanout: self.dropped_on_fanout.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time view of [`Pulse`]'s internal counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PulseMetricsSnapshot {
    /// Total events passed to [`Pulse::ingest`].
    pub ingested: u64,

    /// Events that passed validation, were not duplicates, and were fanned
    /// out (to zero or more subscribers). Does not include coalesced
    /// events; those count under `coalescing` until `flush_coalesced`.
    pub accepted: u64,

    /// Events dropped because their id was already in the dedup cache.
    pub deduped: u64,

    /// Events buffered by the coalesce map on ingest. Each increment
    /// corresponds to one `IngestResult::Coalescing`.
    pub coalescing: u64,

    /// Events actually delivered by [`Pulse::flush_coalesced`].
    pub coalesced_flushed: u64,

    /// Events rejected for failing [`is_valid_kind`].
    ///
    /// [`is_valid_kind`]: super::is_valid_kind
    pub rejected_invalid_kind: u64,

    /// Events rejected because their namespace is not registered.
    pub rejected_unregistered_namespace: u64,

    /// Events rejected because `event.kind != event.payload.kind()`.
    pub rejected_kind_payload_mismatch: u64,

    /// Occasions where `broadcast::Sender::send` returned `Err` (no
    /// receivers attached). The event was still accepted by Pulse; the
    /// downstream simply had no listener.
    pub dropped_on_fanout: u64,
}

// ---------------------------------------------------------------------------
// Dedup cache (bounded FIFO)
// ---------------------------------------------------------------------------

/// Bounded FIFO-evicting set of event ids.
///
/// Not a strict LRU — on `contains` we don't move the id to the back.
/// That's correct for dedup: we care "have we seen this id in the last N
/// events?", and FIFO insert-order is sufficient. Strict LRU would cost
/// extra work for no semantic gain.
struct DedupCache {
    ids: HashSet<EventId>,
    order: VecDeque<EventId>,
    capacity: usize,
}

impl DedupCache {
    fn new(capacity: usize) -> Self {
        let cap = capacity.max(1); // avoid divide-by-zero / no-op cache
        Self {
            ids: HashSet::with_capacity(cap),
            order: VecDeque::with_capacity(cap),
            capacity: cap,
        }
    }

    /// Insert `id` into the cache. Returns `true` if the id was already
    /// present (the caller should treat the event as a duplicate).
    fn insert(&mut self, id: EventId) -> bool {
        if !self.ids.insert(id) {
            // already present
            return true;
        }
        self.order.push_back(id);
        if self.order.len() > self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Pulse
// ---------------------------------------------------------------------------

/// The orchestrator. See the [module docs] for responsibilities and
/// [COMPANION-0003] for the book ADR.
///
/// # Thread safety
///
/// `Pulse` is `Send + Sync`. All mutating state is behind `std::sync::Mutex`
/// or `RwLock` with tiny critical sections. None of its methods are async
/// and none hold a sync mutex across an `.await`.
///
/// [module docs]: crate::garden::pulse
/// [COMPANION-0003]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0003-pulse.md
pub struct Pulse {
    subscribers: broadcast::Sender<Event>,
    dedup: Mutex<DedupCache>,
    coalesce: Mutex<HashMap<&'static str, Event>>,
    namespaces: RwLock<HashSet<&'static str>>,
    metrics: Arc<PulseMetrics>,
}

impl Pulse {
    /// Construct a `Pulse` with the given configuration.
    pub fn new(config: PulseConfig) -> Self {
        let (subscribers, _) = broadcast::channel(config.broadcast_capacity);
        Self {
            subscribers,
            dedup: Mutex::new(DedupCache::new(config.dedup_capacity)),
            coalesce: Mutex::new(HashMap::new()),
            namespaces: RwLock::new(HashSet::new()),
            metrics: Arc::new(PulseMetrics::default()),
        }
    }

    /// Construct a `Pulse` with default capacities. Equivalent to
    /// `Pulse::new(PulseConfig::default())`.
    pub fn with_defaults() -> Self {
        Self::new(PulseConfig::default())
    }

    /// Register a namespace prefix as acceptable. Events whose `kind`
    /// does not begin with a registered namespace (followed by `.`) are
    /// rejected with [`RejectReason::UnregisteredNamespace`].
    ///
    /// Registration is idempotent — registering the same namespace twice
    /// is a no-op.
    pub fn register_namespace(&self, ns: &'static str) {
        self.namespaces
            .write()
            .expect("Pulse namespaces lock poisoned")
            .insert(ns);
    }

    /// The single fan-in point. Validates the event, checks the dedup
    /// cache, either buffers (if coalescing) or fans out.
    pub fn ingest(&self, event: Event) -> IngestResult {
        self.metrics.ingested.fetch_add(1, Ordering::Relaxed);

        // 1. Syntactic kind validation.
        if !is_valid_kind(event.kind) {
            self.metrics
                .rejected_invalid_kind
                .fetch_add(1, Ordering::Relaxed);
            return IngestResult::Rejected(RejectReason::InvalidKindFormat);
        }

        // 2. Namespace registration. `kind_namespace` cannot fail here
        //    because `is_valid_kind` already established there are at
        //    least three dot-separated parts, but we handle None defensively.
        let ns = match kind_namespace(event.kind) {
            Some(ns) => ns,
            None => {
                self.metrics
                    .rejected_invalid_kind
                    .fetch_add(1, Ordering::Relaxed);
                return IngestResult::Rejected(RejectReason::InvalidKindFormat);
            }
        };
        {
            let ns_set = self
                .namespaces
                .read()
                .expect("Pulse namespaces lock poisoned");
            if !ns_set.contains(ns) {
                self.metrics
                    .rejected_unregistered_namespace
                    .fetch_add(1, Ordering::Relaxed);
                return IngestResult::Rejected(RejectReason::UnregisteredNamespace);
            }
        }

        // 3. Kind/payload coherence. The envelope's `kind` field must
        //    equal what the payload reports via `DynPayload::kind`.
        if event.kind != event.payload.kind() {
            self.metrics
                .rejected_kind_payload_mismatch
                .fetch_add(1, Ordering::Relaxed);
            return IngestResult::Rejected(RejectReason::KindPayloadMismatch);
        }

        // 4. Dedup by event id.
        {
            let mut cache = self.dedup.lock().expect("Pulse dedup lock poisoned");
            if cache.insert(event.id) {
                // already present
                self.metrics.deduped.fetch_add(1, Ordering::Relaxed);
                return IngestResult::Duplicate;
            }
        }

        // 5. Coalesce (latest-wins) or fan out immediately.
        if event.payload.is_coalescing() {
            self.metrics.coalescing.fetch_add(1, Ordering::Relaxed);
            self.coalesce
                .lock()
                .expect("Pulse coalesce lock poisoned")
                .insert(event.kind, event);
            IngestResult::Coalescing
        } else {
            self.fan_out(event, /*as_coalesced_flush=*/ false)
        }
    }

    /// Drain the coalesce buffer, emitting each kept event to subscribers.
    /// Returns the number of events flushed.
    ///
    /// Typically called on a timer by `Companion::run` (Book VII). Also
    /// callable directly for tests or step-through scenarios.
    pub fn flush_coalesced(&self) -> usize {
        // Drain under the lock; send after releasing.
        let drained: Vec<Event> = {
            let mut map = self
                .coalesce
                .lock()
                .expect("Pulse coalesce lock poisoned");
            map.drain().map(|(_, e)| e).collect()
        };
        let count = drained.len();
        for event in drained {
            let _ = self.fan_out(event, /*as_coalesced_flush=*/ true);
        }
        count
    }

    /// Subscribe to the canonical event stream.
    ///
    /// Subscribers that fall behind the broadcast capacity receive
    /// `RecvError::Lagged(skipped)` and are expected to recover by
    /// re-reading current state from the `Garden` aggregate (Book V).
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.subscribers.subscribe()
    }

    /// Point-in-time snapshot of all pulse counters.
    pub fn metrics(&self) -> PulseMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Current number of attached subscribers.
    pub fn receiver_count(&self) -> usize {
        self.subscribers.receiver_count()
    }

    // --- internal ---

    fn fan_out(&self, event: Event, as_coalesced_flush: bool) -> IngestResult {
        match self.subscribers.send(event) {
            Ok(n) => {
                if as_coalesced_flush {
                    self.metrics
                        .coalesced_flushed
                        .fetch_add(1, Ordering::Relaxed);
                } else {
                    self.metrics.accepted.fetch_add(1, Ordering::Relaxed);
                }
                IngestResult::Accepted { subscribers: n }
            }
            Err(_) => {
                // No receivers — the event is discarded by tokio, but Pulse
                // did its job. The operator can see this via the metric.
                self.metrics
                    .dropped_on_fanout
                    .fetch_add(1, Ordering::Relaxed);
                if as_coalesced_flush {
                    // coalesced_flushed counts "pulse attempted to flush",
                    // not "a receiver got it". Increment here too so the
                    // flush metric matches flush_coalesced()'s return value.
                    self.metrics
                        .coalesced_flushed
                        .fetch_add(1, Ordering::Relaxed);
                } else {
                    self.metrics.accepted.fetch_add(1, Ordering::Relaxed);
                }
                IngestResult::Accepted { subscribers: 0 }
            }
        }
    }
}

impl std::fmt::Debug for Pulse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pulse")
            .field("receiver_count", &self.receiver_count())
            .field("metrics", &self.metrics())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{Event, EventPayload};
    use std::any::Any;

    // --- Test payloads ---

    #[derive(Debug)]
    struct Tended;
    impl EventPayload for Tended {
        const KIND: &'static str = "core.stone.tended";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug)]
    struct LoadUpdated {
        cpu: u8,
    }
    impl EventPayload for LoadUpdated {
        const KIND: &'static str = "core.stone.load.updated";
        const COALESCING: bool = true;
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug)]
    struct HealthChanged;
    impl EventPayload for HealthChanged {
        const KIND: &'static str = "core.stone.health.changed";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug)]
    struct FireflyBrightness;
    impl EventPayload for FireflyBrightness {
        const KIND: &'static str = "firefly.command.brightness";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn pulse_with_core() -> Pulse {
        let pulse = Pulse::with_defaults();
        pulse.register_namespace("core");
        pulse
    }

    // --- Happy path ---

    #[test]
    fn accepts_valid_event_and_fans_out() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        let result = pulse.ingest(Event::new(Tended));

        assert!(matches!(
            result,
            IngestResult::Accepted { subscribers: 1 }
        ));
        let delivered = rx.try_recv().expect("receiver should have event");
        assert_eq!(delivered.kind, "core.stone.tended");
    }

    #[test]
    fn ingest_with_no_subscribers_reports_zero_subscribers() {
        let pulse = pulse_with_core();

        let result = pulse.ingest(Event::new(Tended));

        assert_eq!(result, IngestResult::Accepted { subscribers: 0 });
        assert_eq!(pulse.metrics().dropped_on_fanout, 1);
    }

    #[test]
    fn subscribers_receive_events_in_order() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        pulse.ingest(Event::new(Tended));
        pulse.ingest(Event::new(HealthChanged));
        pulse.ingest(Event::new(Tended));

        assert_eq!(rx.try_recv().unwrap().kind, Tended::KIND);
        assert_eq!(rx.try_recv().unwrap().kind, HealthChanged::KIND);
        assert_eq!(rx.try_recv().unwrap().kind, Tended::KIND);
    }

    // --- Validation ---

    #[test]
    fn rejects_invalid_kind_format() {
        let pulse = pulse_with_core();

        // Build an Event with a malformed kind by going through the
        // public struct fields. Kind doesn't conform to the grammar.
        let evt = Event {
            id: super::super::new_event_id(),
            timestamp: chrono::Utc::now(),
            kind: "BAD",
            payload: Arc::new(Tended),
        };

        let result = pulse.ingest(evt);
        assert_eq!(
            result,
            IngestResult::Rejected(RejectReason::InvalidKindFormat)
        );
        assert_eq!(pulse.metrics().rejected_invalid_kind, 1);
    }

    #[test]
    fn rejects_unregistered_namespace() {
        let pulse = pulse_with_core(); // only "core" registered

        let result = pulse.ingest(Event::new(FireflyBrightness));

        assert_eq!(
            result,
            IngestResult::Rejected(RejectReason::UnregisteredNamespace)
        );
        assert_eq!(pulse.metrics().rejected_unregistered_namespace, 1);
    }

    #[test]
    fn register_namespace_accepts_after_rejection() {
        let pulse = pulse_with_core();

        // Before: firefly unregistered → rejected.
        let r1 = pulse.ingest(Event::new(FireflyBrightness));
        assert!(matches!(
            r1,
            IngestResult::Rejected(RejectReason::UnregisteredNamespace)
        ));

        // Register, then retry with a fresh event (new id to avoid dedup).
        pulse.register_namespace("firefly");
        let mut rx = pulse.subscribe();
        let r2 = pulse.ingest(Event::new(FireflyBrightness));
        assert!(matches!(r2, IngestResult::Accepted { subscribers: 1 }));
        assert_eq!(rx.try_recv().unwrap().kind, FireflyBrightness::KIND);
    }

    #[test]
    fn rejects_kind_payload_mismatch() {
        let pulse = pulse_with_core();

        // kind says "tended", payload is HealthChanged.
        let evt = Event {
            id: super::super::new_event_id(),
            timestamp: chrono::Utc::now(),
            kind: "core.stone.tended",
            payload: Arc::new(HealthChanged),
        };

        let result = pulse.ingest(evt);
        assert_eq!(
            result,
            IngestResult::Rejected(RejectReason::KindPayloadMismatch)
        );
        assert_eq!(pulse.metrics().rejected_kind_payload_mismatch, 1);
    }

    // --- Dedup ---

    #[test]
    fn dedupes_by_event_id() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        let evt = Event::new(Tended);
        let id = evt.id;

        let first = pulse.ingest(evt.clone());
        assert!(matches!(first, IngestResult::Accepted { subscribers: 1 }));
        assert_eq!(rx.try_recv().unwrap().id, id);

        // Re-ingesting the SAME event (same id) is a duplicate.
        let second = pulse.ingest(evt);
        assert_eq!(second, IngestResult::Duplicate);
        assert!(rx.try_recv().is_err()); // nothing new delivered
        assert_eq!(pulse.metrics().deduped, 1);
    }

    #[test]
    fn dedup_cache_evicts_fifo_beyond_capacity() {
        // Capacity 3: after 4 distinct ids are inserted, the oldest gets evicted.
        let pulse = Pulse::new(PulseConfig {
            dedup_capacity: 3,
            broadcast_capacity: 16,
        });
        pulse.register_namespace("core");

        let evt1 = Event::new(Tended);
        let evt2 = Event::new(Tended);
        let evt3 = Event::new(Tended);
        let evt4 = Event::new(Tended);

        assert!(matches!(
            pulse.ingest(evt1.clone()),
            IngestResult::Accepted { .. }
        ));
        assert!(matches!(
            pulse.ingest(evt2.clone()),
            IngestResult::Accepted { .. }
        ));
        assert!(matches!(
            pulse.ingest(evt3.clone()),
            IngestResult::Accepted { .. }
        ));

        // Cache holds {1, 2, 3}. Re-ingesting any is Duplicate.
        assert_eq!(pulse.ingest(evt1.clone()), IngestResult::Duplicate);

        // Fourth event → cache holds {2, 3, 4}; id1 was evicted.
        assert!(matches!(
            pulse.ingest(evt4),
            IngestResult::Accepted { .. }
        ));

        // id2 is still present; id1 is gone.
        assert_eq!(pulse.ingest(evt2), IngestResult::Duplicate);
        assert!(matches!(
            pulse.ingest(evt1),
            IngestResult::Accepted { .. }
        ));
    }

    // --- Coalescing ---

    #[test]
    fn coalesces_events_flagged_coalescing() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        let result = pulse.ingest(Event::new(LoadUpdated { cpu: 42 }));

        assert_eq!(result, IngestResult::Coalescing);
        // Nothing fanned out yet.
        assert!(rx.try_recv().is_err());
        assert_eq!(pulse.metrics().coalescing, 1);
        assert_eq!(pulse.metrics().accepted, 0);
    }

    #[test]
    fn non_coalescing_events_bypass_buffer() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        pulse.ingest(Event::new(Tended));

        // Delivered immediately, not buffered.
        assert_eq!(rx.try_recv().unwrap().kind, Tended::KIND);
        assert_eq!(pulse.metrics().coalescing, 0);
        assert_eq!(pulse.flush_coalesced(), 0);
    }

    #[test]
    fn coalesce_keeps_latest_per_kind() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        pulse.ingest(Event::new(LoadUpdated { cpu: 10 }));
        pulse.ingest(Event::new(LoadUpdated { cpu: 20 }));
        pulse.ingest(Event::new(LoadUpdated { cpu: 30 }));

        assert_eq!(pulse.metrics().coalescing, 3);
        assert_eq!(pulse.flush_coalesced(), 1); // only latest survives

        let delivered = rx.try_recv().unwrap();
        let payload = delivered.payload::<LoadUpdated>().unwrap();
        assert_eq!(payload.cpu, 30);
    }

    #[test]
    fn flush_coalesced_emits_pending_and_returns_count() {
        let pulse = pulse_with_core();
        let mut rx = pulse.subscribe();

        // Two different coalescing kinds simultaneously buffered.
        // For this test we only have LoadUpdated as coalescing, so two
        // of those coalesce to one, giving a count of 1.
        pulse.ingest(Event::new(LoadUpdated { cpu: 1 }));
        pulse.ingest(Event::new(LoadUpdated { cpu: 2 }));

        let count = pulse.flush_coalesced();
        assert_eq!(count, 1);

        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.payload::<LoadUpdated>().unwrap().cpu, 2);
        assert_eq!(pulse.metrics().coalesced_flushed, 1);
    }

    #[test]
    fn flush_coalesced_clears_buffer() {
        let pulse = pulse_with_core();
        let _rx = pulse.subscribe();

        pulse.ingest(Event::new(LoadUpdated { cpu: 1 }));
        assert_eq!(pulse.flush_coalesced(), 1);
        // Second flush — buffer is empty, nothing to emit.
        assert_eq!(pulse.flush_coalesced(), 0);
    }

    // --- Metrics ---

    #[test]
    fn metrics_snapshot_reflects_activity() {
        let pulse = pulse_with_core();
        let _rx = pulse.subscribe();

        pulse.ingest(Event::new(Tended)); // accepted
        let dup = Event::new(Tended);
        pulse.ingest(dup.clone());
        pulse.ingest(dup); // deduped
        pulse.ingest(Event::new(LoadUpdated { cpu: 5 })); // coalescing
        pulse.ingest(Event::new(FireflyBrightness)); // rejected: unregistered ns

        let m = pulse.metrics();
        assert_eq!(m.ingested, 5);
        assert_eq!(m.accepted, 2);
        assert_eq!(m.deduped, 1);
        assert_eq!(m.coalescing, 1);
        assert_eq!(m.rejected_unregistered_namespace, 1);
        assert_eq!(m.rejected_invalid_kind, 0);
        assert_eq!(m.rejected_kind_payload_mismatch, 0);
    }

    #[test]
    fn receiver_count_reflects_active_subscribers() {
        let pulse = pulse_with_core();
        assert_eq!(pulse.receiver_count(), 0);

        let rx1 = pulse.subscribe();
        assert_eq!(pulse.receiver_count(), 1);

        let rx2 = pulse.subscribe();
        assert_eq!(pulse.receiver_count(), 2);

        drop(rx1);
        assert_eq!(pulse.receiver_count(), 1);

        drop(rx2);
        assert_eq!(pulse.receiver_count(), 0);
    }
}
