//! Domain event for the Metrics aggregate.
//!
//! Per ARCH-0018: events fire only on **interesting transitions**
//! (task state changes, lag detection, registration). Counter
//! increments (`record_domain_event`, `record_mutation_latency`) do
//! **not** fire events — they would flood the broadcast channel
//! under load. Consumers that want counter values poll the snapshot
//! endpoint instead.

use chrono::{DateTime, Utc};

/// Event emitted by the Metrics aggregate on interesting transitions.
///
/// Note: unlike other aggregate events in moss, `MetricsChanged` has
/// **no `affected: Vec<String>` field**. Metrics observes other
/// contexts and has no per-item identity of its own — events describe
/// global transitions, not per-item changes. This is documented in
/// ARCH-0018 as an explicit deviation from the pattern spec.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MetricsChanged {
    /// A domain was registered with the aggregate. Fired once per
    /// domain, typically at bootstrap.
    DomainRegistered { domain: &'static str },

    /// A task was registered with the aggregate. Fired once per task,
    /// typically when the task supervisor spawns it.
    TaskRegistered { task: &'static str },

    /// A task signaled ready for the first time. Fired once per task.
    TaskReady {
        task: &'static str,
        ready_at: DateTime<Utc>,
    },

    /// A task transitioned to a new lifecycle state (e.g., Running →
    /// Completed). Fired whenever a transition is recorded.
    TaskStateChanged {
        task: &'static str,
        state: &'static str,
    },

    /// A projection task observed `broadcast::RecvError::Lagged`,
    /// meaning it fell behind the producer. Fired on each lag event.
    SubscriberLagDetected { task: &'static str, skipped: u64 },
}
