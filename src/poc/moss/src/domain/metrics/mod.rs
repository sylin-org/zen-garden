//! Metrics bounded context — stone self-observation.
//!
//! Metrics is a pure observer context. It holds per-domain counters,
//! per-task observability data, and process-global counters behind a
//! `RwLock`, exposes typed query methods for the read surface, and
//! publishes `MetricsChanged` events on interesting transitions.
//!
//! Hot-path recording (`record_domain_event`, `record_mutation_latency`,
//! `record_subscriber_lag`) is lock-free after the initial map lookup —
//! atomic counters live inside `Arc<DomainMetrics>` / `Arc<TaskMetrics>`
//! values that the aggregate clones out of its state map before
//! releasing the read lock.
//!
//! See [ARCH-0018](../../../../../docs/decisions/ARCH-0018-metrics-aggregate.md)
//! for the full decision record, including three documented deviations
//! from the standard domain aggregate pattern spec (no persistence
//! port, infallible mutations, no `affected` field on events).

pub mod aggregate;
pub mod error;
pub mod event;
pub mod snapshot;
pub mod state;

#[cfg(test)]
mod tests;

pub use aggregate::Metrics;
pub use error::MetricsError;
pub use event::MetricsChanged;
pub use snapshot::{
    DomainSnapshot, GlobalSnapshot, LatencySnapshot, MetricsSnapshot, TaskSnapshot,
};
// `state` types (DomainMetrics, TaskMetrics, GlobalMetrics,
// LatencyHistogram, MetricsState) are intentionally not re-exported at
// the module root — they are internal implementation types holding
// atomics, never handed out to callers. Consumers use the `*Snapshot`
// value types instead.
