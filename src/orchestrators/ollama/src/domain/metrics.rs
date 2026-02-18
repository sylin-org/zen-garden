//! In-memory metrics engine with ring-buffer response times.
//!
//! All mutation happens through `record_*` methods. The flush task
//! periodically snapshots the state to JSON on disk.

use super::types::{MetricsSnapshot, StoneMetrics};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Maximum number of response-time samples in the ring buffer.
const RING_CAPACITY: usize = 1000;

/// Live metrics state (owned by AppState behind a RwLock).
#[derive(Debug)]
pub struct MetricsEngine {
    pub requests_total: u64,
    pub tokens_in_total: u64,
    pub tokens_out_total: u64,
    pub errors_total: u64,
    pub per_stone: HashMap<String, StoneMetrics>,
    pub per_model: HashMap<String, u64>,
    /// (timestamp, duration_ns) ring buffer for recent response times.
    pub response_times: VecDeque<(Instant, u64)>,
    pub started_at: Instant,
    pub enabled: bool,
}

impl MetricsEngine {
    pub fn new() -> Self {
        Self {
            requests_total: 0,
            tokens_in_total: 0,
            tokens_out_total: 0,
            errors_total: 0,
            per_stone: HashMap::new(),
            per_model: HashMap::new(),
            response_times: VecDeque::with_capacity(RING_CAPACITY),
            started_at: Instant::now(),
            enabled: true,
        }
    }

    /// Record a successful inference response.
    pub fn record_request(
        &mut self,
        stone_name: &str,
        model: &str,
        tokens_in: u64,
        tokens_out: u64,
        duration_ns: u64,
    ) {
        if !self.enabled {
            return;
        }

        self.requests_total += 1;
        self.tokens_in_total += tokens_in;
        self.tokens_out_total += tokens_out;

        *self.per_model.entry(model.to_string()).or_default() += 1;

        let stone = self
            .per_stone
            .entry(stone_name.to_string())
            .or_default();
        stone.requests += 1;
        stone.tokens_in += tokens_in;
        stone.tokens_out += tokens_out;
        stone.total_duration_ns += duration_ns;

        // Ring buffer
        if self.response_times.len() >= RING_CAPACITY {
            self.response_times.pop_front();
        }
        self.response_times.push_back((Instant::now(), duration_ns));
    }

    /// Record a failed request.
    pub fn record_error(&mut self, stone_name: &str) {
        if !self.enabled {
            return;
        }
        self.errors_total += 1;
        self.per_stone
            .entry(stone_name.to_string())
            .or_default()
            .errors += 1;
    }

    /// Average response time over the last N seconds.
    pub fn avg_response_ns(&self, window_secs: u64) -> Option<u64> {
        let cutoff = Instant::now() - std::time::Duration::from_secs(window_secs);
        let recent: Vec<u64> = self
            .response_times
            .iter()
            .filter(|(t, _)| *t > cutoff)
            .map(|(_, d)| *d)
            .collect();
        if recent.is_empty() {
            return None;
        }
        Some(recent.iter().sum::<u64>() / recent.len() as u64)
    }

    /// Request count in the last N seconds.
    pub fn requests_in_window(&self, window_secs: u64) -> u64 {
        let cutoff = Instant::now() - std::time::Duration::from_secs(window_secs);
        self.response_times
            .iter()
            .filter(|(t, _)| *t > cutoff)
            .count() as u64
    }

    /// Top models by request count.
    pub fn top_models(&self, n: usize) -> Vec<(String, u64)> {
        let mut sorted: Vec<_> = self.per_model.iter().map(|(k, v)| (k.clone(), *v)).collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(n);
        sorted
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        self.requests_total = 0;
        self.tokens_in_total = 0;
        self.tokens_out_total = 0;
        self.errors_total = 0;
        self.per_stone.clear();
        self.per_model.clear();
        self.response_times.clear();
        self.started_at = Instant::now();
    }

    /// Produce a serializable snapshot for persistence.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            requests_total: self.requests_total,
            tokens_in_total: self.tokens_in_total,
            tokens_out_total: self.tokens_out_total,
            errors_total: self.errors_total,
            per_stone: self.per_stone.clone(),
            per_model: self.per_model.clone(),
            started_at: Some(
                chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::from_std(self.started_at.elapsed()).unwrap_or_default())
                    .unwrap_or_else(chrono::Utc::now)
                    .to_rfc3339(),
            ),
            snapshot_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

impl Default for MetricsEngine {
    fn default() -> Self {
        Self::new()
    }
}
