//! Serialize-able value types exposed through the Metrics read API.
//!
//! Snapshot types are plain value structs — no atomics, no shared
//! references, no interior mutability. They are produced by the
//! aggregate's query methods (`snapshot`, `domain`, `task`, etc.) as
//! cloned copies of current state and carry enough context to be
//! returned directly by HTTP handlers.
//!
//! These types are intentionally distinct from `state.rs` types
//! (which hold atomics for lock-free hot-path recording) so the
//! "hot-path vs wire-format" boundary is explicit.

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Full snapshot of the Metrics aggregate.
///
/// Returned by `Metrics::snapshot()` and serialized as the body of
/// `GET /api/v1/stone/metrics` (once that endpoint lands in Chapter 5).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub global: GlobalSnapshot,
    pub domains: Vec<DomainSnapshot>,
    pub tasks: Vec<TaskSnapshot>,
}

/// Process-wide snapshot slice.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalSnapshot {
    pub started_at: DateTime<Utc>,
    pub uptime_seconds: i64,
    pub events_total: u64,
    pub lag_total: u64,
}

/// Per-domain snapshot slice.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomainSnapshot {
    pub name: String,
    pub events_total: u64,
    /// Per-kind counts, keyed by stable kind name. `BTreeMap` for
    /// deterministic JSON output.
    pub events_by_kind: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<DateTime<Utc>>,
    pub mutation_latency: LatencySnapshot,
    pub subscribers_lagged_total: u64,
}

/// Latency histogram snapshot.
///
/// Prometheus-compatible shape: total count, cumulative sum,
/// and bucket counts keyed by upper-bound label.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencySnapshot {
    pub count: u64,
    pub total_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_ms: Option<f64>,
    /// Bucket counts keyed by their "le" label (`"1ms"`, `"5ms"`, ...,
    /// `"+Inf"`). `BTreeMap` for deterministic ordering. Values are
    /// non-cumulative counts — each bucket holds the number of
    /// observations that fell in that range.
    pub buckets: BTreeMap<String, u64>,
}

/// Per-task snapshot slice.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskSnapshot {
    pub name: String,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_at: Option<DateTime<Utc>>,
    pub events_received_total: u64,
    pub events_lagged_total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<DateTime<Utc>>,
}
