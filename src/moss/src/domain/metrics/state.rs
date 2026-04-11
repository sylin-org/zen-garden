//! Internal state of the Metrics aggregate.
//!
//! The types in this file own atomics and are designed for lock-free
//! hot-path recording. They are **not** `Serialize` — see `snapshot.rs`
//! for the Serialize-able value types exposed through the aggregate's
//! read API.
//!
//! ## Lock-free hot path
//!
//! The recording flow is:
//!
//! 1. Acquire a read lock on `MetricsState` (shared, short).
//! 2. Look up `domains.get(name)` → `Option<&Arc<DomainMetrics>>`.
//! 3. Clone the Arc and drop the read lock.
//! 4. Increment atomics on the cloned `Arc<DomainMetrics>` — no lock.
//!
//! `events_by_kind` is a plain `HashMap` populated once at
//! `register_domain` time and never mutated again. Lookups are
//! therefore lock-free even without an interior sync primitive.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64};

/// Observation buckets (milliseconds) for the latency histogram.
///
/// Prometheus-style le-bucket boundaries. Values ≤ bucket are counted.
/// An extra "+Inf" bucket at index [BUCKET_BOUNDS.len()] catches
/// overflow.
pub(super) const BUCKET_BOUNDS_MS: [u64; 8] = [1, 5, 10, 50, 100, 500, 1000, 5000];

/// Human-readable bucket labels for the snapshot format.
pub(super) const BUCKET_LABELS: [&str; 9] = [
    "1ms", "5ms", "10ms", "50ms", "100ms", "500ms", "1s", "5s", "+Inf",
];

/// Fixed-bucket latency histogram with lock-free atomic counters.
///
/// Prometheus-histogram-compatible. `record` increments the count,
/// adds to the total, and increments exactly one bucket.
#[derive(Debug)]
pub struct LatencyHistogram {
    pub count: AtomicU64,
    pub total_ms: AtomicU64,
    pub buckets: [AtomicU64; 9],
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            total_ms: AtomicU64::new(0),
            buckets: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// Record one observation. Increments count, total_ms, and the
    /// appropriate bucket. Lock-free.
    pub fn record(&self, elapsed: std::time::Duration) {
        use std::sync::atomic::Ordering;

        let ms = elapsed.as_millis() as u64;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_ms.fetch_add(ms, Ordering::Relaxed);

        // Find the first bucket whose bound is >= ms.
        // If none match, fall into the +Inf overflow bucket.
        let idx = BUCKET_BOUNDS_MS
            .iter()
            .position(|&bound| ms <= bound)
            .unwrap_or(BUCKET_BOUNDS_MS.len());
        self.buckets[idx].fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-domain observability counters.
///
/// Created once at `Metrics::register_domain` and stored inside
/// `MetricsState::domains`. All fields are atomic — increments are
/// lock-free on the hot path.
#[derive(Debug)]
pub struct DomainMetrics {
    /// Total events emitted by this domain, across all kinds.
    pub events_total: AtomicU64,

    /// Per-kind event counts. Populated at registration with a static
    /// list of possible kinds and never mutated afterward. Lookups
    /// are lock-free (no interior sync needed because the map is
    /// effectively immutable after registration).
    pub events_by_kind: HashMap<&'static str, AtomicU64>,

    /// Milliseconds-since-epoch of the most recent event. Zero if
    /// none recorded.
    pub last_event_at_ms: AtomicI64,

    /// Latency of the `finalize` pipeline (persist + meter + emit)
    /// for this domain.
    pub mutation_latency: LatencyHistogram,

    /// Count of subscriber-lag events observed on this domain's
    /// broadcast channel. Incremented by projection tasks that see
    /// `RecvError::Lagged`.
    pub subscribers_lagged_total: AtomicU64,
}

impl DomainMetrics {
    pub(super) fn new(kinds: &'static [&'static str]) -> Arc<Self> {
        let mut events_by_kind = HashMap::with_capacity(kinds.len());
        for kind in kinds {
            events_by_kind.insert(*kind, AtomicU64::new(0));
        }

        Arc::new(Self {
            events_total: AtomicU64::new(0),
            events_by_kind,
            last_event_at_ms: AtomicI64::new(0),
            mutation_latency: LatencyHistogram::new(),
            subscribers_lagged_total: AtomicU64::new(0),
        })
    }
}

/// Per-task observability counters.
///
/// Created at `Metrics::register_task`. Tracks timing (`started_at`,
/// `ready_at`) and event flow for projection tasks.
#[derive(Debug)]
pub struct TaskMetrics {
    /// Wall clock when the task was registered (typically its spawn time).
    pub started_at: DateTime<Utc>,

    /// Wall clock when the task called `ctx.ready.signal()`, stored as
    /// milliseconds-since-epoch. Zero if the task has not yet signaled.
    pub ready_at_ms: AtomicI64,

    /// Cumulative events received by this task (if it is a projection
    /// subscriber that publishes to Metrics).
    pub events_received_total: AtomicU64,

    /// Cumulative lag events (`RecvError::Lagged`) observed by this task.
    pub events_lagged_total: AtomicU64,

    /// Milliseconds-since-epoch of the most recent event received.
    /// Zero if no events recorded.
    pub last_event_at_ms: AtomicI64,
}

impl TaskMetrics {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at: Utc::now(),
            ready_at_ms: AtomicI64::new(0),
            events_received_total: AtomicU64::new(0),
            events_lagged_total: AtomicU64::new(0),
            last_event_at_ms: AtomicI64::new(0),
        })
    }
}

/// Global, process-wide counters.
#[derive(Debug)]
pub struct GlobalMetrics {
    /// Process start time (set at aggregate construction).
    pub started_at: DateTime<Utc>,

    /// Sum of all domain events across all contexts.
    pub events_total: AtomicU64,

    /// Sum of subscriber lag events across all tasks.
    pub lag_total: AtomicU64,
}

impl GlobalMetrics {
    pub(super) fn new() -> Self {
        Self {
            started_at: Utc::now(),
            events_total: AtomicU64::new(0),
            lag_total: AtomicU64::new(0),
        }
    }
}

/// Internal state of the `Metrics` aggregate.
///
/// Kept behind a single `RwLock` on `Metrics`. The lock is held for
/// very short intervals on the hot path — long enough to look up the
/// per-domain or per-task `Arc` and then released. Actual counter
/// increments happen on the cloned `Arc` without the lock.
pub(super) struct MetricsState {
    pub(super) global: GlobalMetrics,
    pub(super) domains: HashMap<&'static str, Arc<DomainMetrics>>,
    pub(super) tasks: HashMap<&'static str, Arc<TaskMetrics>>,
}

impl MetricsState {
    pub(super) fn new() -> Self {
        Self {
            global: GlobalMetrics::new(),
            domains: HashMap::new(),
            tasks: HashMap::new(),
        }
    }
}
