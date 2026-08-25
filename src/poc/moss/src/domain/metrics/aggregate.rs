//! The Metrics aggregate — DDD root for stone self-observation.
//!
//! Metrics is a bounded context that observes other contexts. It
//! holds per-domain counters, per-task observability data, and
//! process-global counters privately, exposes typed query methods
//! for the read surface, and publishes `MetricsChanged` events on
//! interesting transitions.
//!
//! See [ARCH-0018](../../../../../docs/decisions/ARCH-0018-metrics-aggregate.md)
//! for full rationale. Three documented deviations from the pattern
//! spec:
//!
//! 1. **No `Store` port.** Metrics is in-memory only; counters reset
//!    on restart (Prometheus-standard behavior).
//! 2. **Infallible mutations.** Recording methods return `()`, not
//!    `Result`. A metrics recording failure must never break the
//!    caller's hot path.
//! 3. **No `affected` field on events.** Metrics observes other
//!    contexts and has no per-item identity of its own.

use chrono::{DateTime, TimeZone, Utc};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};

use super::event::MetricsChanged;
use super::snapshot::{
    DomainSnapshot, GlobalSnapshot, LatencySnapshot, MetricsSnapshot, TaskSnapshot,
};
use super::state::{BUCKET_LABELS, DomainMetrics, MetricsState, TaskMetrics};

/// The Metrics bounded context's aggregate root.
pub struct Metrics {
    state: RwLock<MetricsState>,
    changes: broadcast::Sender<MetricsChanged>,
}

impl Metrics {
    /// Stable context name — used as the key when this context
    /// observes itself (future: metrics recording its own operations).
    pub const NAME: &'static str = "metrics";

    /// Construct an empty Metrics aggregate with no registered
    /// domains or tasks.
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(garden_common::constants::channels::METRICS_EVENT);
        Self {
            state: RwLock::new(MetricsState::new()),
            changes,
        }
    }

    // ========================================================================
    // Event subscription
    // ========================================================================

    /// Subscribe to aggregate-level mutation events.
    ///
    /// Subscribers receive a `MetricsChanged` only on interesting
    /// transitions. Counter increments do **not** fire events. See
    /// [`MetricsChanged`] for the full list of event kinds.
    pub fn changes(&self) -> broadcast::Receiver<MetricsChanged> {
        self.changes.subscribe()
    }

    // ========================================================================
    // Mutation API — infallible, no Result
    // ========================================================================

    /// Register a domain context with Metrics.
    ///
    /// `kinds` enumerates the stable `&'static str` names of every
    /// possible event kind for this domain (e.g., `"upserted"`,
    /// `"removed"`, `"promoted"`). These names are populated into a
    /// read-only per-kind counter map at registration time, enabling
    /// lock-free per-kind increments on the hot path.
    ///
    /// Calling `register_domain` twice with the same name is a
    /// **no-op** — the existing entry is preserved and no event
    /// fires. This makes bootstrap code idempotent.
    #[tracing::instrument(level = "debug", skip(self, kinds), fields(metrics.domain = %name))]
    pub async fn register_domain(&self, name: &'static str, kinds: &'static [&'static str]) {
        {
            let mut st = self.state.write().await;
            if st.domains.contains_key(name) {
                return;
            }
            st.domains.insert(name, DomainMetrics::new(kinds));
        }
        let _ = self
            .changes
            .send(MetricsChanged::DomainRegistered { domain: name });
    }

    /// Register a background task with Metrics. Idempotent.
    #[tracing::instrument(level = "debug", skip(self), fields(metrics.task = %name))]
    pub async fn register_task(&self, name: &'static str) {
        {
            let mut st = self.state.write().await;
            if st.tasks.contains_key(name) {
                return;
            }
            st.tasks.insert(name, TaskMetrics::new());
        }
        let _ = self
            .changes
            .send(MetricsChanged::TaskRegistered { task: name });
    }

    /// Record a domain mutation event.
    ///
    /// Hot-path recording: takes a read lock on state, clones the
    /// `Arc<DomainMetrics>`, drops the lock, then increments atomics.
    /// Does **not** fire a `MetricsChanged` event — counter
    /// increments are observed via snapshot polling, not via the
    /// broadcast channel.
    ///
    /// Silently no-op if the domain is not registered. A tracing
    /// warn-level log fires in that case.
    #[tracing::instrument(level = "trace", skip(self), fields(metrics.domain = %domain, metrics.kind = %kind))]
    pub async fn record_domain_event(&self, domain: &'static str, kind: &'static str) {
        let Some(dm) = self.domain_arc(domain).await else {
            tracing::warn!(
                domain,
                kind,
                "Metrics::record_domain_event called for unregistered domain"
            );
            return;
        };

        dm.events_total.fetch_add(1, Ordering::Relaxed);
        if let Some(counter) = dm.events_by_kind.get(kind) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            tracing::warn!(
                domain,
                kind,
                "Metrics::record_domain_event called with unregistered kind"
            );
        }
        dm.last_event_at_ms.store(now_ms(), Ordering::Relaxed);

        // Also increment the global counter.
        {
            let st = self.state.read().await;
            st.global.events_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a mutation latency observation for a domain.
    ///
    /// Hot-path recording. Silently no-op if the domain is not
    /// registered (with a tracing warn log).
    #[tracing::instrument(level = "trace", skip(self), fields(metrics.domain = %domain))]
    pub async fn record_mutation_latency(&self, domain: &'static str, elapsed: Duration) {
        let Some(dm) = self.domain_arc(domain).await else {
            tracing::warn!(
                domain,
                "Metrics::record_mutation_latency called for unregistered domain"
            );
            return;
        };
        dm.mutation_latency.record(elapsed);
    }

    /// Record that a task signaled ready for the first time.
    ///
    /// Publishes `MetricsChanged::TaskReady`. Idempotent: if the task
    /// was already marked ready, no event fires.
    #[tracing::instrument(level = "debug", skip(self), fields(metrics.task = %task))]
    pub async fn record_task_ready(&self, task: &'static str) {
        let Some(tm) = self.task_arc(task).await else {
            tracing::warn!(
                task,
                "Metrics::record_task_ready called for unregistered task"
            );
            return;
        };

        let now = now_ms();
        // Compare-and-swap the first time only.
        let previous = tm.ready_at_ms.swap(now, Ordering::Relaxed);
        if previous != 0 {
            // Already marked ready; don't fire a duplicate event.
            return;
        }

        let ready_at = Utc
            .timestamp_millis_opt(now)
            .single()
            .unwrap_or_else(Utc::now);
        let _ = self
            .changes
            .send(MetricsChanged::TaskReady { task, ready_at });
    }

    /// Record a task lifecycle state transition.
    ///
    /// Publishes `MetricsChanged::TaskStateChanged`. Metrics does not
    /// own task state — this method exists to broadcast lifecycle
    /// events for consumers that care (dashboard, alerting).
    #[tracing::instrument(level = "debug", skip(self), fields(metrics.task = %task, metrics.state = %state))]
    pub async fn record_task_transition(&self, task: &'static str, state: &'static str) {
        // We don't actually store task state — just check the task is
        // registered so we get a warn log if misused, then fire the
        // event.
        if self.task_arc(task).await.is_none() {
            tracing::warn!(
                task,
                state,
                "Metrics::record_task_transition called for unregistered task"
            );
            return;
        }
        let _ = self
            .changes
            .send(MetricsChanged::TaskStateChanged { task, state });
    }

    /// Record that a projection task observed `broadcast::RecvError::Lagged`.
    ///
    /// Publishes `MetricsChanged::SubscriberLagDetected` and
    /// increments both the per-task and per-global lag counters.
    #[tracing::instrument(level = "debug", skip(self), fields(metrics.task = %task, metrics.skipped = skipped))]
    pub async fn record_subscriber_lag(&self, task: &'static str, skipped: u64) {
        let Some(tm) = self.task_arc(task).await else {
            tracing::warn!(
                task,
                skipped,
                "Metrics::record_subscriber_lag called for unregistered task"
            );
            return;
        };
        tm.events_lagged_total.fetch_add(skipped, Ordering::Relaxed);

        {
            let st = self.state.read().await;
            st.global.lag_total.fetch_add(skipped, Ordering::Relaxed);
        }

        let _ = self
            .changes
            .send(MetricsChanged::SubscriberLagDetected { task, skipped });
    }

    // ========================================================================
    // Read API
    // ========================================================================

    /// Full observability snapshot.
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let st = self.state.read().await;
        MetricsSnapshot {
            global: snapshot_global(&st.global),
            domains: st
                .domains
                .iter()
                .map(|(name, dm)| snapshot_domain(name, dm))
                .collect(),
            tasks: st
                .tasks
                .iter()
                .map(|(name, tm)| snapshot_task(name, tm))
                .collect(),
        }
    }

    /// Process-wide counters only.
    pub async fn global(&self) -> GlobalSnapshot {
        let st = self.state.read().await;
        snapshot_global(&st.global)
    }

    /// Snapshot of all registered domains.
    pub async fn domains(&self) -> Vec<DomainSnapshot> {
        let st = self.state.read().await;
        st.domains
            .iter()
            .map(|(name, dm)| snapshot_domain(name, dm))
            .collect()
    }

    /// Snapshot of a single domain. Returns None if the name is not
    /// registered.
    pub async fn domain(&self, name: &str) -> Option<DomainSnapshot> {
        let st = self.state.read().await;
        st.domains
            .get_key_value(name)
            .map(|(n, dm)| snapshot_domain(n, dm))
    }

    /// Snapshot of all registered tasks.
    pub async fn tasks(&self) -> Vec<TaskSnapshot> {
        let st = self.state.read().await;
        st.tasks
            .iter()
            .map(|(name, tm)| snapshot_task(name, tm))
            .collect()
    }

    /// Snapshot of a single task. Returns None if the name is not
    /// registered.
    pub async fn task(&self, name: &str) -> Option<TaskSnapshot> {
        let st = self.state.read().await;
        st.tasks
            .get_key_value(name)
            .map(|(n, tm)| snapshot_task(n, tm))
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Clone the `Arc<DomainMetrics>` for a domain, dropping the read
    /// lock before returning. Used by hot-path recording methods so
    /// atomic increments happen outside the lock scope.
    async fn domain_arc(&self, name: &str) -> Option<Arc<DomainMetrics>> {
        let st = self.state.read().await;
        st.domains.get(name).cloned()
    }

    /// Clone the `Arc<TaskMetrics>` for a task, dropping the read lock.
    async fn task_arc(&self, name: &str) -> Option<Arc<TaskMetrics>> {
        let st = self.state.read().await;
        st.tasks.get(name).cloned()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

// ========================================================================
// Snapshot helpers — pure functions that copy atomic state into value types
// ========================================================================

fn snapshot_global(g: &super::state::GlobalMetrics) -> GlobalSnapshot {
    let uptime = (Utc::now() - g.started_at).num_seconds();
    GlobalSnapshot {
        started_at: g.started_at,
        uptime_seconds: uptime,
        events_total: g.events_total.load(Ordering::Relaxed),
        lag_total: g.lag_total.load(Ordering::Relaxed),
    }
}

fn snapshot_domain(name: &'static str, dm: &Arc<DomainMetrics>) -> DomainSnapshot {
    let events_total = dm.events_total.load(Ordering::Relaxed);

    let mut events_by_kind = BTreeMap::new();
    for (kind, counter) in &dm.events_by_kind {
        events_by_kind.insert((*kind).to_string(), counter.load(Ordering::Relaxed));
    }

    let last_event_at = ms_to_datetime(dm.last_event_at_ms.load(Ordering::Relaxed));

    let latency_count = dm.mutation_latency.count.load(Ordering::Relaxed);
    let latency_total_ms = dm.mutation_latency.total_ms.load(Ordering::Relaxed);
    let mean_ms = if latency_count > 0 {
        Some(latency_total_ms as f64 / latency_count as f64)
    } else {
        None
    };

    let mut buckets = BTreeMap::new();
    for (label, counter) in BUCKET_LABELS.iter().zip(dm.mutation_latency.buckets.iter()) {
        buckets.insert((*label).to_string(), counter.load(Ordering::Relaxed));
    }

    DomainSnapshot {
        name: name.to_string(),
        events_total,
        events_by_kind,
        last_event_at,
        mutation_latency: LatencySnapshot {
            count: latency_count,
            total_ms: latency_total_ms,
            mean_ms,
            buckets,
        },
        subscribers_lagged_total: dm.subscribers_lagged_total.load(Ordering::Relaxed),
    }
}

fn snapshot_task(name: &'static str, tm: &Arc<TaskMetrics>) -> TaskSnapshot {
    let ready_at = ms_to_datetime(tm.ready_at_ms.load(Ordering::Relaxed));
    let last_event_at = ms_to_datetime(tm.last_event_at_ms.load(Ordering::Relaxed));

    TaskSnapshot {
        name: name.to_string(),
        started_at: tm.started_at,
        ready_at,
        events_received_total: tm.events_received_total.load(Ordering::Relaxed),
        events_lagged_total: tm.events_lagged_total.load(Ordering::Relaxed),
        last_event_at,
    }
}

// ========================================================================
// Time helpers
// ========================================================================

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn ms_to_datetime(ms: i64) -> Option<DateTime<Utc>> {
    if ms == 0 {
        None
    } else {
        Utc.timestamp_millis_opt(ms).single()
    }
}
