//! Demand Ledger: exponentially decayed request tracking.
//!
//! Pure domain — no I/O, no async, no locks. All types here support
//! the demand-weighted topology advisor (ORCH-0009).
//!
//! The ledger maintains three decay windows per counter:
//! - **Reactive** (15 min half-life): parallelism adjustment, queue pressure
//! - **Tactical** (6 hour half-life): placement optimization, workload separation
//! - **Strategic** (3 day half-life): replication suggestions, eviction candidates
//!
//! Each `DecayCounter` is ~24 bytes. No histograms, no time-bucketed storage.

use std::collections::HashMap;
use std::time::Instant;

use crate::domain::types::Capability;

// ── Decay Windows ───────────────────────────────────────────────

/// Half-life for the reactive window (parallelism, queue pressure).
const REACTIVE_HALF_LIFE_SECS: f64 = 15.0 * 60.0; // 15 minutes

/// Half-life for the tactical window (placement, workload separation).
const TACTICAL_HALF_LIFE_SECS: f64 = 6.0 * 3600.0; // 6 hours

/// Half-life for the strategic window (replication, eviction).
const STRATEGIC_HALF_LIFE_SECS: f64 = 3.0 * 86400.0; // 3 days

/// Minimum requests before demand weights override uniform assumption.
const CONFIDENCE_THRESHOLD: f64 = 50.0;

/// ln(2), used in exponential decay computation.
const LN2: f64 = std::f64::consts::LN_2;

// ── DecayCounter ────────────────────────────────────────────────

/// Exponentially weighted event counter.
///
/// Records events and computes a decayed rate (events per hour) at query
/// time. Old events are naturally forgotten without explicit expiry.
///
/// The decay formula: `rate = count × 2^(-elapsed / half_life)`
/// Each `record()` adds 1.0 to the accumulated count after decaying
/// the existing total to the current time.
#[derive(Debug, Clone)]
pub struct DecayCounter {
    /// Accumulated decayed count.
    value: f64,
    /// When `value` was last updated.
    last_update: Instant,
}

impl DecayCounter {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            last_update: Instant::now(),
        }
    }

    /// Record one event at the given timestamp.
    pub fn record(&mut self, now: Instant) {
        self.decay_to(now);
        self.value += 1.0;
    }

    /// Record N events at the given timestamp.
    pub fn record_n(&mut self, now: Instant, n: f64) {
        self.decay_to(now);
        self.value += n;
    }

    /// Query the decayed count at the given time with the specified half-life.
    ///
    /// This is the "effective recent event count" — higher half-life means
    /// older events contribute more.
    pub fn count(&self, now: Instant, half_life_secs: f64) -> f64 {
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.value * (-LN2 * elapsed / half_life_secs).exp()
    }

    /// Query the decayed rate in events per hour.
    pub fn rate_per_hour(&self, now: Instant, half_life_secs: f64) -> f64 {
        // The decayed count represents "effective recent events".
        // To convert to a rate, divide by the half-life (in hours)
        // scaled by ln(2) to get the average rate.
        let count = self.count(now, half_life_secs);
        count * LN2 / (half_life_secs / 3600.0)
    }

    fn decay_to(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        if elapsed > 0.0 {
            // Use the tactical half-life as the internal decay rate.
            // Queries with different half-lives re-scale at read time.
            self.value *= (-LN2 * elapsed / TACTICAL_HALF_LIFE_SECS).exp();
            self.last_update = now;
        }
    }
}

impl Default for DecayCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ── DecayAverage ────────────────────────────────────────────────

/// Exponentially weighted moving average for continuous values (e.g. tok/s).
#[derive(Debug, Clone)]
pub struct DecayAverage {
    /// Weighted sum of values.
    weighted_sum: f64,
    /// Weighted count of observations.
    weighted_count: f64,
    /// When last updated.
    last_update: Instant,
}

impl DecayAverage {
    pub fn new() -> Self {
        Self {
            weighted_sum: 0.0,
            weighted_count: 0.0,
            last_update: Instant::now(),
        }
    }

    /// Record an observation.
    pub fn record(&mut self, now: Instant, value: f64) {
        self.decay_to(now);
        self.weighted_sum += value;
        self.weighted_count += 1.0;
    }

    /// Query the decayed average at the given time.
    pub fn average(&self, now: Instant, half_life_secs: f64) -> Option<f64> {
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        let factor = (-LN2 * elapsed / half_life_secs).exp();
        let count = self.weighted_count * factor;
        if count < 0.1 {
            return None; // Not enough data
        }
        Some(self.weighted_sum * factor / count)
    }

    fn decay_to(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        if elapsed > 0.0 {
            let factor = (-LN2 * elapsed / REACTIVE_HALF_LIFE_SECS).exp();
            self.weighted_sum *= factor;
            self.weighted_count *= factor;
            self.last_update = now;
        }
    }
}

impl Default for DecayAverage {
    fn default() -> Self {
        Self::new()
    }
}

// ── DemandLedger ────────────────────────────────────────────────

/// Aggregated demand tracking for the topology advisor.
///
/// Fed by the metrics processor on every request. Read by the advisor
/// to compute capability pressure and demand-weighted placement.
#[derive(Debug)]
pub struct DemandLedger {
    /// Per-capability request counters.
    pub by_capability: HashMap<Capability, DecayCounter>,

    /// Per-model request counters.
    pub by_model: HashMap<String, DecayCounter>,

    /// Per-(model, stone) observed throughput (tok/s).
    pub observed_fitness: HashMap<(String, String), DecayAverage>,

    /// Per-(model, stone) cold-load events.
    pub cold_loads: HashMap<(String, String), DecayCounter>,

    /// Total requests recorded (for confidence ramp).
    pub total_requests: u64,
}

impl DemandLedger {
    pub fn new() -> Self {
        Self {
            by_capability: HashMap::new(),
            by_model: HashMap::new(),
            observed_fitness: HashMap::new(),
            cold_loads: HashMap::new(),
            total_requests: 0,
        }
    }

    /// Record a successful inference request.
    pub fn record_request(
        &mut self,
        now: Instant,
        capability: Capability,
        model: &str,
        stone: &str,
        tokens_out: u64,
        eval_duration_ns: u64,
    ) {
        self.total_requests += 1;

        self.by_capability
            .entry(capability)
            .or_default()
            .record(now);

        self.by_model
            .entry(model.to_string())
            .or_default()
            .record(now);

        // Record observed fitness (tok/s) if we have generation data
        if tokens_out > 0 && eval_duration_ns > 0 {
            let tps = tokens_out as f64 / (eval_duration_ns as f64 / 1_000_000_000.0);
            self.observed_fitness
                .entry((model.to_string(), stone.to_string()))
                .or_default()
                .record(now, tps);
        }
    }

    /// Record a cold-load event (model loaded from disk to VRAM).
    pub fn record_cold_load(&mut self, now: Instant, model: &str, stone: &str) {
        self.cold_loads
            .entry((model.to_string(), stone.to_string()))
            .or_default()
            .record(now);
    }

    /// Confidence factor: 0.0 (no data) to 1.0 (sufficient data).
    ///
    /// Below 1.0, the advisor blends observed demand with uniform weights.
    pub fn confidence(&self) -> f64 {
        (self.total_requests as f64 / CONFIDENCE_THRESHOLD).min(1.0)
    }

    /// Per-capability demand distribution.
    ///
    /// Returns capability → share (0.0..1.0). Blends observed demand
    /// with uniform distribution based on confidence level.
    pub fn capability_distribution(&self, now: Instant) -> HashMap<Capability, f64> {
        let confidence = self.confidence();
        let n_capabilities = Capability::ALL.len() as f64;
        let uniform_weight = 1.0 / n_capabilities;

        if confidence < f64::EPSILON {
            // No data: pure uniform
            return Capability::ALL
                .iter()
                .map(|&c| (c, uniform_weight))
                .collect();
        }

        // Compute observed shares from tactical window
        let mut observed: HashMap<Capability, f64> = HashMap::new();
        let mut total = 0.0f64;
        for (&cap, counter) in &self.by_capability {
            let count = counter.count(now, TACTICAL_HALF_LIFE_SECS);
            observed.insert(cap, count);
            total += count;
        }

        // Normalize observed to shares
        if total > 0.0 {
            for val in observed.values_mut() {
                *val /= total;
            }
        }

        // Blend: lerp(uniform, observed, confidence)
        Capability::ALL
            .iter()
            .map(|&cap| {
                let obs = observed.get(&cap).copied().unwrap_or(0.0);
                let blended = uniform_weight * (1.0 - confidence) + obs * confidence;
                (cap, blended)
            })
            .collect()
    }

    /// Per-model demand shares (tactical window).
    ///
    /// Returns model → share (0.0..1.0).
    pub fn model_distribution(&self, now: Instant) -> HashMap<String, f64> {
        let mut counts: HashMap<&str, f64> = HashMap::new();
        let mut total = 0.0f64;
        for (model, counter) in &self.by_model {
            let count = counter.count(now, TACTICAL_HALF_LIFE_SECS);
            if count > 0.01 {
                counts.insert(model.as_str(), count);
                total += count;
            }
        }
        if total == 0.0 {
            return HashMap::new();
        }
        counts
            .into_iter()
            .map(|(m, c)| (m.to_string(), c / total))
            .collect()
    }

    /// Observed fitness (tok/s) for a model on a stone (reactive window).
    pub fn observed_tps(&self, model: &str, stone: &str) -> Option<f64> {
        self.observed_fitness
            .get(&(model.to_string(), stone.to_string()))
            .and_then(|avg| avg.average(Instant::now(), REACTIVE_HALF_LIFE_SECS))
    }

    /// Cold-load rate for a model on a stone (tactical window, events/hour).
    pub fn cold_load_rate(&self, model: &str, stone: &str) -> f64 {
        self.cold_loads
            .get(&(model.to_string(), stone.to_string()))
            .map(|c| c.rate_per_hour(Instant::now(), TACTICAL_HALF_LIFE_SECS))
            .unwrap_or(0.0)
    }

    /// Reactive-window request rate per capability (requests/hour).
    pub fn capability_rates(&self, now: Instant) -> HashMap<Capability, f64> {
        self.by_capability
            .iter()
            .map(|(&cap, counter)| {
                (cap, counter.rate_per_hour(now, REACTIVE_HALF_LIFE_SECS))
            })
            .collect()
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        self.by_capability.clear();
        self.by_model.clear();
        self.observed_fitness.clear();
        self.cold_loads.clear();
        self.total_requests = 0;
    }
}

impl Default for DemandLedger {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn decay_counter_basic() {
        let start = Instant::now();
        let mut counter = DecayCounter::new();

        // Record 10 events
        for _ in 0..10 {
            counter.record(start);
        }

        // At t=0, count should be ~10
        let count = counter.count(start, 3600.0);
        assert!((count - 10.0).abs() < 0.01, "expected ~10, got {count}");

        // After one half-life, count should be ~5
        let later = start + Duration::from_secs(3600);
        let count = counter.count(later, 3600.0);
        assert!(
            (count - 5.0).abs() < 0.5,
            "expected ~5 after one half-life, got {count}"
        );
    }

    #[test]
    fn decay_counter_rate() {
        let start = Instant::now();
        let mut counter = DecayCounter::new();
        counter.record(start);

        let rate = counter.rate_per_hour(start, 3600.0);
        // With 1 event and 1h half-life, rate ≈ ln(2) ≈ 0.693 events/hour
        assert!(rate > 0.5 && rate < 1.0, "expected ~0.693, got {rate}");
    }

    #[test]
    fn demand_ledger_confidence_ramp() {
        let ledger = DemandLedger::new();
        assert!((ledger.confidence() - 0.0).abs() < f64::EPSILON);

        let mut ledger = DemandLedger::new();
        let now = Instant::now();
        for _ in 0..25 {
            ledger.record_request(now, Capability::Chat, "llama3:8b", "stone-a", 100, 1_000_000_000);
        }
        assert!((ledger.confidence() - 0.5).abs() < 0.01);

        for _ in 0..25 {
            ledger.record_request(now, Capability::Chat, "llama3:8b", "stone-a", 100, 1_000_000_000);
        }
        assert!((ledger.confidence() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn capability_distribution_uniform_at_zero() {
        let ledger = DemandLedger::new();
        let dist = ledger.capability_distribution(Instant::now());

        let n = Capability::ALL.len() as f64;
        for &cap in Capability::ALL {
            let share = dist.get(&cap).copied().unwrap_or(0.0);
            assert!(
                (share - 1.0 / n).abs() < 0.01,
                "{cap}: expected uniform {}, got {share}",
                1.0 / n
            );
        }
    }

    #[test]
    fn capability_distribution_converges() {
        let mut ledger = DemandLedger::new();
        let now = Instant::now();

        // Record 100 embedding requests (above confidence threshold)
        for _ in 0..100 {
            ledger.record_request(now, Capability::Embed, "nomic-embed", "stone-a", 0, 0);
        }

        let dist = ledger.capability_distribution(now);
        let embed_share = dist.get(&Capability::Embed).copied().unwrap_or(0.0);
        // Should be dominant (close to 1.0 since confidence is 1.0)
        assert!(
            embed_share > 0.8,
            "expected embed > 0.8, got {embed_share}"
        );
    }

    #[test]
    fn observed_fitness_tracking() {
        let mut ledger = DemandLedger::new();
        let now = Instant::now();

        // Record requests with 50 tok/s
        for _ in 0..5 {
            ledger.record_request(
                now,
                Capability::Chat,
                "llama3:8b",
                "stone-a",
                50,         // 50 tokens
                1_000_000_000, // 1 second
            );
        }

        let tps = ledger.observed_tps("llama3:8b", "stone-a");
        assert!(tps.is_some());
        let tps = tps.unwrap();
        assert!(
            (tps - 50.0).abs() < 1.0,
            "expected ~50 tok/s, got {tps}"
        );
    }
}
