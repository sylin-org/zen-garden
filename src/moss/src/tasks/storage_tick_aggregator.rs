//! Storage tick aggregator (STORAGE-0006 Phase 4f)
//!
//! Quantizes raw `StorageTick` events into aggregated ticks for downstream
//! consumers (SSE stream, replication task).
//!
//! ## Problem
//!
//! A single `PUT /object` writes two files (content + `.meta.json` sidecar),
//! each producing a raw `StorageTick` on the `storage_tick` broadcast
//! channel.  A batch of N objects therefore fires 2N raw events.  Without
//! aggregation, the SSE stream would spam subscribers and the replication
//! task would kick off 2N sync cycles — most of which are redundant.
//!
//! ## Solution — Per-Seed-Bank Quantization
//!
//! ```text
//!  ┌──────────────┐   raw ticks    ┌────────────────────┐  agg ticks   ┌───────────┐
//!  │ ContentStore ├──────────────►│ StorageTickAggregator├────────────►│ SSE stream│
//!  │  (per write)  │               │  (per seed bank)    │             └───────────┘
//!  └──────────────┘               │                     │  agg ticks   ┌───────────┐
//!                                  │  2s quiet / 10s cap ├────────────►│ Replication│
//!                                  └────────────────────┘             └───────────┘
//! ```
//!
//! The aggregator subscribes to the raw `storage_tick` channel and
//! maintains **independent** quantization state per seed bank name.
//!
//! ### Flush policy (per seed bank)
//!
//! | Condition | Description |
//! |-----------|-------------|
//! | **Quiet threshold** (2 s) | No raw tick received for 2 seconds → flush |
//! | **Deadline cap** (10 s) | Time since first unconsumed tick exceeds 10 seconds → flush |
//!
//! Whichever fires first emits a single aggregated `StorageTick` on the
//! cooked `storage_agg` channel with cumulative `C`/`M`/`D` counts and
//! the latest cursor.  The per-bank state is then reset.
//!
//! ### Timing
//!
//! A 250 ms poll loop checks all banks for expired thresholds.  This keeps
//! flush latency bounded at ≤ 250 ms beyond the threshold while consuming
//! trivial CPU.
//!
//! ## Channel wiring
//!
//! ```text
//! storage_tick  ──►  [aggregator task]  ──►  storage_agg
//!                                                  ├──► SSE /api/v1/stone/storage/stream
//!                                                  └──► storage_replication_task
//! ```
//!
//! Raw channel (`storage_tick`) is **internal-only** — downstream
//! consumers must subscribe to the aggregated channel.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use garden_common::storage::StorageTick;

// ============================================================================
// Constants
// ============================================================================

/// No raw tick for this long → flush the accumulated counters.
const QUIET_THRESHOLD: Duration = Duration::from_secs(2);

/// Maximum time since the first un-flushed tick before a forced flush.
const DEADLINE_CAP: Duration = Duration::from_secs(10);

/// How often we poll for expired thresholds across all banks.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

// ============================================================================
// Per-bank aggregation state
// ============================================================================

/// Accumulator for a single seed bank's raw tick window.
struct BankWindow {
    /// Cumulative create count in this window.
    creates: u32,
    /// Cumulative modify count in this window.
    modifies: u32,
    /// Cumulative delete count in this window.
    deletes: u32,
    /// Latest cursor seen (carries through to the emitted tick).
    cursor: String,
    /// Replica set ID (STORAGE-0013). Propagated from the raw tick.
    replica_set_id: String,
    /// When the first raw tick in this window arrived (for deadline cap).
    window_start: Instant,
    /// When the most recent raw tick arrived (for quiet threshold).
    last_event: Instant,
}

impl BankWindow {
    fn new(tick: &StorageTick, now: Instant) -> Self {
        Self {
            creates: tick.creates,
            modifies: tick.modifies,
            deletes: tick.deletes,
            cursor: tick.cursor.clone(),
            replica_set_id: tick.replica_set_id.clone(),
            window_start: now,
            last_event: now,
        }
    }

    /// Fold another raw tick into this window.
    fn accumulate(&mut self, tick: &StorageTick, now: Instant) {
        self.creates += tick.creates;
        self.modifies += tick.modifies;
        self.deletes += tick.deletes;
        self.cursor = tick.cursor.clone();
        if !tick.replica_set_id.is_empty() {
            self.replica_set_id = tick.replica_set_id.clone();
        }
        self.last_event = now;
    }

    /// Build the aggregated tick for emission.
    fn to_tick(&self, seed_bank: &str) -> StorageTick {
        StorageTick {
            cursor: self.cursor.clone(),
            storage: seed_bank.to_string(),
            replica_set_id: self.replica_set_id.clone(),
            creates: self.creates,
            modifies: self.modifies,
            deletes: self.deletes,
        }
    }

    /// Returns `true` if the quiet threshold or deadline cap has been reached.
    fn should_flush(&self, now: Instant) -> bool {
        now.duration_since(self.last_event) >= QUIET_THRESHOLD
            || now.duration_since(self.window_start) >= DEADLINE_CAP
    }
}

// ============================================================================
// Public entry point
// ============================================================================

/// Background task — spawned at daemon startup.
///
/// Subscribes to the raw `storage_tick` channel and emits quantized
/// aggregated ticks on `agg_tx`.  Runs for the daemon's entire lifetime.
pub async fn storage_tick_aggregator_task(
    mut raw_rx: broadcast::Receiver<StorageTick>,
    agg_tx: broadcast::Sender<StorageTick>,
    token: CancellationToken,
) {
    info!("Storage tick aggregator started (2s quiet / 10s deadline)");

    let mut banks: HashMap<String, BankWindow> = HashMap::new();
    let mut poll = tokio::time::interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            // ── Raw tick received ──────────────────────────────────────
            result = raw_rx.recv() => {
                match result {
                    Ok(tick) => {
                        let now = Instant::now();
                        banks
                            .entry(tick.storage.clone())
                            .and_modify(|w| w.accumulate(&tick, now))
                            .or_insert_with(|| BankWindow::new(&tick, now));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(lagged = n, "Aggregator lagged on raw tick channel");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Raw tick channel closed — aggregator exiting");
                        // Flush any remaining windows before exit
                        flush_all(&mut banks, &agg_tx);
                        return;
                    }
                }
            }

            // ── Poll timer — check for expired windows ─────────────────
            _ = poll.tick() => {
                let now = Instant::now();
                let ready: Vec<String> = banks
                    .iter()
                    .filter(|(_, w)| w.should_flush(now))
                    .map(|(name, _)| name.clone())
                    .collect();

                for name in ready {
                    if let Some(window) = banks.remove(&name) {
                        emit(&agg_tx, &name, &window);
                    }
                }
            }

            // ── Shutdown ───────────────────────────────────────────────
            _ = token.cancelled() => {
                info!("Storage tick aggregator shutting down");
                flush_all(&mut banks, &agg_tx);
                return;
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Emit a single aggregated tick on the cooked channel.
fn emit(tx: &broadcast::Sender<StorageTick>, name: &str, window: &BankWindow) {
    let tick = window.to_tick(name);
    debug!(
        seed_bank = %name,
        cursor = %tick.cursor,
        creates = tick.creates,
        modifies = tick.modifies,
        deletes = tick.deletes,
        "Flushing aggregated storage tick"
    );
    // send() fails only if there are zero receivers — that's fine.
    let _ = tx.send(tick);
}

/// Drain all pending windows (used on shutdown / channel close).
fn flush_all(banks: &mut HashMap<String, BankWindow>, tx: &broadcast::Sender<StorageTick>) {
    for (name, window) in banks.drain() {
        emit(tx, &name, &window);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick(storage: &str, creates: u32, modifies: u32, deletes: u32) -> StorageTick {
        StorageTick {
            cursor: format!("cursor-{}", creates + modifies + deletes),
            storage: storage.to_string(),
            replica_set_id: String::new(),
            creates,
            modifies,
            deletes,
        }
    }

    // ── BankWindow::new ────────────────────────────────────────────────

    #[test]
    fn test_bank_window_new_captures_tick() {
        let tick = make_tick("photos", 1, 0, 0);
        let now = Instant::now();
        let window = BankWindow::new(&tick, now);

        assert_eq!(window.creates, 1);
        assert_eq!(window.modifies, 0);
        assert_eq!(window.deletes, 0);
        assert_eq!(window.cursor, "cursor-1");
        assert_eq!(window.window_start, now);
        assert_eq!(window.last_event, now);
    }

    // ── BankWindow::accumulate ─────────────────────────────────────────

    #[test]
    fn test_accumulate_sums_counts() {
        let tick1 = make_tick("photos", 2, 1, 0);
        let now = Instant::now();
        let mut window = BankWindow::new(&tick1, now);

        let tick2 = make_tick("photos", 3, 0, 1);
        let later = now + Duration::from_millis(500);
        window.accumulate(&tick2, later);

        assert_eq!(window.creates, 5);
        assert_eq!(window.modifies, 1);
        assert_eq!(window.deletes, 1);
    }

    #[test]
    fn test_accumulate_updates_cursor() {
        let tick1 = make_tick("data", 1, 0, 0);
        let now = Instant::now();
        let mut window = BankWindow::new(&tick1, now);

        let tick2 = StorageTick {
            cursor: "latest-cursor".to_string(),
            storage: "data".to_string(),
            replica_set_id: String::new(),
            creates: 0,
            modifies: 1,
            deletes: 0,
        };
        window.accumulate(&tick2, now + Duration::from_millis(100));

        assert_eq!(window.cursor, "latest-cursor");
    }

    #[test]
    fn test_accumulate_updates_last_event_not_window_start() {
        let tick1 = make_tick("data", 1, 0, 0);
        let t0 = Instant::now();
        let mut window = BankWindow::new(&tick1, t0);

        let t1 = t0 + Duration::from_secs(1);
        let tick2 = make_tick("data", 0, 1, 0);
        window.accumulate(&tick2, t1);

        assert_eq!(window.window_start, t0, "window_start should not change");
        assert_eq!(window.last_event, t1, "last_event should advance");
    }

    // ── BankWindow::to_tick ────────────────────────────────────────────

    #[test]
    fn test_to_tick_projects_correctly() {
        let tick = make_tick("backups", 5, 3, 2);
        let window = BankWindow::new(&tick, Instant::now());

        let agg = window.to_tick("backups");
        assert_eq!(agg.storage, "backups");
        assert_eq!(agg.creates, 5);
        assert_eq!(agg.modifies, 3);
        assert_eq!(agg.deletes, 2);
        assert_eq!(agg.cursor, "cursor-10");
    }

    // ── BankWindow::should_flush ───────────────────────────────────────

    #[test]
    fn test_should_flush_false_when_fresh() {
        let tick = make_tick("data", 1, 0, 0);
        let now = Instant::now();
        let window = BankWindow::new(&tick, now);

        // Immediately after creation — neither threshold met
        assert!(!window.should_flush(now));
        assert!(!window.should_flush(now + Duration::from_secs(1)));
    }

    #[test]
    fn test_should_flush_true_after_quiet_threshold() {
        let tick = make_tick("data", 1, 0, 0);
        let now = Instant::now();
        let window = BankWindow::new(&tick, now);

        // Exactly at quiet threshold (2s)
        assert!(window.should_flush(now + QUIET_THRESHOLD));
        // Past quiet threshold
        assert!(window.should_flush(now + Duration::from_secs(3)));
    }

    #[test]
    fn test_should_flush_true_after_deadline_cap() {
        let tick = make_tick("data", 1, 0, 0);
        let t0 = Instant::now();
        let mut window = BankWindow::new(&tick, t0);

        // Keep accumulating within quiet threshold to prevent quiet-based flush
        for i in 1..=9 {
            let tick = make_tick("data", 1, 0, 0);
            window.accumulate(&tick, t0 + Duration::from_secs(i));
        }

        // last_event is at t0+9s, so quiet threshold (2s) not met at t0+10s
        // But deadline cap (10s from window_start) IS met
        assert!(window.should_flush(t0 + DEADLINE_CAP));
    }

    #[test]
    fn test_should_flush_quiet_resets_with_accumulate() {
        let tick = make_tick("data", 1, 0, 0);
        let t0 = Instant::now();
        let mut window = BankWindow::new(&tick, t0);

        // Almost at quiet threshold
        let almost = t0 + Duration::from_millis(1900);
        assert!(!window.should_flush(almost));

        // Accumulate resets last_event
        let tick2 = make_tick("data", 0, 1, 0);
        window.accumulate(&tick2, almost);

        // Now quiet threshold is 2s from the new last_event
        assert!(!window.should_flush(almost + Duration::from_secs(1)));
        assert!(window.should_flush(almost + QUIET_THRESHOLD));
    }

    // ── flush_all ──────────────────────────────────────────────────────

    #[test]
    fn test_flush_all_drains_banks() {
        let (tx, mut rx) = broadcast::channel::<StorageTick>(16);
        let mut banks = HashMap::new();

        let tick1 = make_tick("alpha", 1, 0, 0);
        let tick2 = make_tick("beta", 0, 2, 0);
        let now = Instant::now();
        banks.insert("alpha".to_string(), BankWindow::new(&tick1, now));
        banks.insert("beta".to_string(), BankWindow::new(&tick2, now));

        flush_all(&mut banks, &tx);

        assert!(banks.is_empty(), "banks should be drained");

        // Should have received 2 aggregated ticks
        let mut received = vec![];
        while let Ok(t) = rx.try_recv() {
            received.push(t.storage);
        }
        received.sort();
        assert_eq!(received, vec!["alpha", "beta"]);
    }

    // ── Constants ──────────────────────────────────────────────────────

    #[test]
    fn test_constants_ordering() {
        assert!(QUIET_THRESHOLD < DEADLINE_CAP, "quiet < deadline");
        assert!(POLL_INTERVAL < QUIET_THRESHOLD, "poll < quiet");
    }
}
