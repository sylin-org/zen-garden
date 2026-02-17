//! Storage tick aggregator (STORAGE-0006 Phase 4f)
//!
//! Quantizes raw `StorageTick` events into aggregated ticks for downstream
//! consumers (SSE stream, replication task).
//!
//! ## Problem
//!
//! A single `PUT /object` writes two files (content + `.meta.json` sidecar),
//! each producing a raw `StorageTick` on the `storage_tick_tx` broadcast
//! channel.  A batch of N objects therefore fires 2N raw events.  Without
//! aggregation, the SSE stream would spam subscribers and the replication
//! task would kick off 2N sync cycles — most of which are redundant.
//!
//! ## Solution — Per-Seed-Bank Quantization
//!
//! ```text
//!  ┌──────────────┐   raw ticks    ┌────────────────────┐  agg ticks   ┌───────────┐
//!  │ SeedBankStore ├──────────────►│ StorageTickAggregator├────────────►│ SSE stream│
//!  │  (per write)  │               │  (per seed bank)    │             └───────────┘
//!  └──────────────┘               │                     │  agg ticks   ┌───────────┐
//!                                  │  2s quiet / 10s cap ├────────────►│ Replication│
//!                                  └────────────────────┘             └───────────┘
//! ```
//!
//! The aggregator subscribes to the raw `storage_tick_tx` channel and
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
//! cooked `storage_agg_tx` channel with cumulative `C`/`M`/`D` counts and
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
//! storage_tick_tx  ──►  [aggregator task]  ──►  storage_agg_tx
//!                                                  ├──► SSE /api/v1/stone/storage/stream
//!                                                  └──► seed_bank_replication_task
//! ```
//!
//! Raw channel (`storage_tick_tx`) is **internal-only** — downstream
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
        self.last_event = now;
    }

    /// Build the aggregated tick for emission.
    fn to_tick(&self, seed_bank: &str) -> StorageTick {
        StorageTick {
            cursor: self.cursor.clone(),
            seed_bank: seed_bank.to_string(),
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
/// Subscribes to the raw `storage_tick_tx` channel and emits quantized
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
                            .entry(tick.seed_bank.clone())
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
