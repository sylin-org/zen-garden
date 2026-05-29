//! `Capacity` aggregate — the disk-space governor (STORAGE-0020).
//!
//! Owns one invariant: free space on the filesystem holding `data_dir()`
//! stays above the survival floor. It holds the *policy* (watermarks,
//! pressure classification, admission decisions) and *orchestrates*
//! reclamation, but never deletes data itself — each consumer keeps its
//! own deletion logic behind the [`Reclaimable`] port.
//!
//! Two entry points:
//! - [`reserve`](Capacity::reserve) — admission control before a large
//!   write. Fails safe: denies when the floor would be breached.
//! - [`govern`](Capacity::govern) — one measure → classify → reclaim cycle,
//!   driven periodically by the `CapacityReclaimTask`.

mod budget;
mod event;
mod reclaimable;
pub mod reclaimers;

pub use budget::{Budget, Pressure, ReclaimLevel};
pub use event::{CapacityChanged, PRESSURE_KINDS};
pub use reclaimable::{Reclaimable, ReclaimPriority, Reclaimed, ReserveRequest, Verdict};

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{broadcast, watch};

use crate::domain::Metrics;
use crate::infra::storage::platform::disk_usage;

/// Capacity of the `CapacityChanged` broadcast channel.
const CHANGES_CHANNEL_CAPACITY: usize = 32;

/// One reclaim pass across all registered reclaimers.
#[derive(Debug, Clone, Serialize)]
pub struct ReclaimRun {
    /// Aggressiveness this pass ran at.
    pub level: ReclaimLevel,
    /// Per-reclaimer outcomes, in the order they were asked.
    pub outcomes: Vec<Reclaimed>,
}

impl ReclaimRun {
    /// Total items removed across all reclaimers this pass.
    pub fn total_items(&self) -> usize {
        self.outcomes.iter().map(|o| o.items_removed).sum()
    }
}

/// Outcome of one [`Capacity::govern`] cycle, for logging and notifications.
#[derive(Debug, Clone)]
pub struct GovernReport {
    /// Pressure observed this cycle (last known if measurement failed).
    pub pressure: Pressure,
    /// Filesystem used percentage, `None` if measurement failed.
    pub used_percent: Option<f64>,
    /// Free bytes, `None` if measurement failed.
    pub available_bytes: Option<u64>,
    /// The reclaim pass run this cycle, if pressure warranted one.
    pub reclaim: Option<ReclaimRun>,
}

/// A single filesystem reading.
struct Reading {
    used_percent: f64,
    available_bytes: u64,
}

/// The disk-space governor.
pub struct Capacity {
    budget: Budget,
    /// Path on the governed filesystem (the data/snapshots filesystem).
    target: String,
    reclaimers: Vec<Arc<dyn Reclaimable>>,
    pressure: watch::Sender<Pressure>,
    changes: broadcast::Sender<CapacityChanged>,
    metrics: Arc<Metrics>,
}

impl Capacity {
    /// Construct the governor for `target` (a path on the filesystem to
    /// govern), with the given policy and reclaimers. Reclaimers are fixed
    /// at construction — the governor never mutates its registry.
    #[tracing::instrument(skip_all, name = "Capacity::new")]
    pub async fn new(
        metrics: Arc<Metrics>,
        target: String,
        budget: Budget,
        reclaimers: Vec<Arc<dyn Reclaimable>>,
    ) -> Self {
        let (pressure, _) = watch::channel(Pressure::Healthy);
        let (changes, _) = broadcast::channel(CHANGES_CHANNEL_CAPACITY);

        metrics.register_domain("capacity", PRESSURE_KINDS).await;

        Self {
            budget,
            target,
            reclaimers,
            pressure,
            changes,
            metrics,
        }
    }

    // ── Admission control ─────────────────────────────────────────────

    /// Decide whether a large write may proceed.
    ///
    /// Denies when the post-write free space would fall below the floor, or
    /// the filesystem is already `Critical`. **Fails open** if free space
    /// cannot be measured — a `df` hiccup must not block every backup.
    pub async fn reserve(&self, request: ReserveRequest) -> Verdict {
        let Some(reading) = self.read().await else {
            tracing::warn!(
                purpose = %request.purpose,
                "capacity: free space unavailable, allowing write (fail-open)"
            );
            return Verdict::Allow;
        };

        let effective_available = reading
            .available_bytes
            .saturating_sub(request.estimated_bytes.unwrap_or(0));

        if self
            .budget
            .deny_admission(reading.used_percent, effective_available)
        {
            let reason = format!(
                "disk at {:.0}% used, {} free (floor {}) — refusing {}",
                reading.used_percent,
                garden_common::format_bytes(reading.available_bytes),
                garden_common::format_bytes(self.budget.min_free_bytes),
                request.purpose
            );
            tracing::warn!(reason = %reason, "capacity: write denied");
            return Verdict::deny(reason);
        }

        Verdict::Allow
    }

    // ── Governing cycle ───────────────────────────────────────────────

    /// One measure → classify → (publish) → reclaim cycle.
    pub async fn govern(&self) -> GovernReport {
        let Some(reading) = self.read().await else {
            tracing::warn!("capacity: measurement failed this cycle, skipping");
            return GovernReport {
                pressure: self.current_pressure(),
                used_percent: None,
                available_bytes: None,
                reclaim: None,
            };
        };

        let pressure = self.budget.classify(reading.used_percent);
        self.publish(pressure, &reading).await;

        let reclaim = match self.budget.reclaim_level(pressure) {
            Some(level) => Some(self.run_reclaim(level).await),
            None => None,
        };

        GovernReport {
            pressure,
            used_percent: Some(reading.used_percent),
            available_bytes: Some(reading.available_bytes),
            reclaim,
        }
    }

    /// Run one reclaim pass at `level`, asking reclaimers in priority order
    /// (`Eager` first). Under real pressure (`Pressure`/`Critical`) it stops
    /// early once free space recovers below the `High` watermark; at
    /// `Routine` it runs every reclaimer (cheap hygiene, no early exit).
    pub async fn run_reclaim(&self, level: ReclaimLevel) -> ReclaimRun {
        let mut ordered: Vec<&Arc<dyn Reclaimable>> = self.reclaimers.iter().collect();
        ordered.sort_by_key(|r| r.priority());

        let early_stop = matches!(level, ReclaimLevel::Pressure | ReclaimLevel::Critical);
        let mut outcomes = Vec::with_capacity(ordered.len());

        for reclaimer in ordered {
            let outcome = reclaimer.reclaim(level).await;
            let freed = outcome.items_removed;
            if !outcome.notes.is_empty() {
                tracing::info!(
                    reclaimer = outcome.reclaimer,
                    notes = ?outcome.notes,
                    "capacity: reclaim outcome"
                );
            }
            outcomes.push(outcome);

            if early_stop && freed > 0 {
                let recovered = self
                    .read()
                    .await
                    .is_some_and(|r| self.budget.classify(r.used_percent) < Pressure::High);
                if recovered {
                    break;
                }
            }
        }

        ReclaimRun { level, outcomes }
    }

    // ── Event API (code standards §13) ────────────────────────────────

    /// Current pressure level (last published).
    pub fn current_pressure(&self) -> Pressure {
        *self.pressure.borrow()
    }

    /// Watch the pressure level — read the current value or await the next
    /// transition.
    pub fn on_pressure_changed(&self) -> watch::Receiver<Pressure> {
        self.pressure.subscribe()
    }

    /// Subscribe to the `CapacityChanged` transition stream.
    pub fn capacity_stream(&self) -> broadcast::Receiver<CapacityChanged> {
        self.changes.subscribe()
    }

    // ── Internals ─────────────────────────────────────────────────────

    /// Publish pressure: update the watch and, on a *transition*, record a
    /// metric and emit a `CapacityChanged` event.
    async fn publish(&self, pressure: Pressure, reading: &Reading) {
        let previous = *self.pressure.borrow();
        self.pressure.send_replace(pressure);

        if previous != pressure {
            self.metrics
                .record_domain_event("capacity", pressure.name())
                .await;
            let _ = self.changes.send(CapacityChanged {
                from: previous,
                to: pressure,
                used_percent: reading.used_percent,
                available_bytes: reading.available_bytes,
                timestamp: chrono::Utc::now(),
            });
            tracing::info!(
                from = previous.name(),
                to = pressure.name(),
                used_percent = reading.used_percent,
                available = %garden_common::format_bytes(reading.available_bytes),
                "capacity: pressure changed"
            );
        }
    }

    /// Measure the governed filesystem off the async runtime (the `df`
    /// subprocess is synchronous). Returns `None` on any failure.
    async fn read(&self) -> Option<Reading> {
        let target = self.target.clone();
        let usage = tokio::task::spawn_blocking(move || disk_usage(&target))
            .await
            .ok()
            .flatten()?;

        let total = usage.used_bytes + usage.available_bytes;
        let used_percent = if total == 0 {
            0.0
        } else {
            (usage.used_bytes as f64 / total as f64) * 100.0
        };

        Some(Reading {
            used_percent,
            available_bytes: usage.available_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    /// Records the order and level at which it is asked to reclaim.
    struct MockReclaimer {
        name: &'static str,
        priority: ReclaimPriority,
        calls: Arc<Mutex<Vec<(&'static str, ReclaimLevel)>>>,
    }

    impl Reclaimable for MockReclaimer {
        fn name(&self) -> &'static str {
            self.name
        }
        fn priority(&self) -> ReclaimPriority {
            self.priority
        }
        fn reclaim<'a>(
            &'a self,
            level: ReclaimLevel,
        ) -> Pin<Box<dyn Future<Output = Reclaimed> + Send + 'a>> {
            let calls = self.calls.clone();
            let name = self.name;
            Box::pin(async move {
                calls.lock().expect("lock").push((name, level));
                Reclaimed::none(name)
            })
        }
    }

    async fn capacity_with(
        target: &str,
        reclaimers: Vec<Arc<dyn Reclaimable>>,
    ) -> Capacity {
        Capacity::new(
            Arc::new(Metrics::new()),
            target.to_string(),
            Budget::default(),
            reclaimers,
        )
        .await
    }

    #[tokio::test]
    async fn run_reclaim_asks_eager_before_normal_and_passes_level() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        // Register Normal first to prove the governor reorders to Eager-first.
        let reclaimers: Vec<Arc<dyn Reclaimable>> = vec![
            Arc::new(MockReclaimer {
                name: "normal",
                priority: ReclaimPriority::Normal,
                calls: calls.clone(),
            }),
            Arc::new(MockReclaimer {
                name: "eager",
                priority: ReclaimPriority::Eager,
                calls: calls.clone(),
            }),
        ];
        // Routine never early-stops, so no disk read happens — deterministic.
        let capacity = capacity_with("/nonexistent-capacity-test-dir", reclaimers).await;
        let run = capacity.run_reclaim(ReclaimLevel::Routine).await;

        let recorded = calls.lock().expect("lock").clone();
        assert_eq!(
            recorded,
            vec![
                ("eager", ReclaimLevel::Routine),
                ("normal", ReclaimLevel::Routine),
            ]
        );
        assert_eq!(run.outcomes.len(), 2);
        assert_eq!(run.total_items(), 0);
    }

    #[tokio::test]
    async fn reserve_fails_open_when_filesystem_unmeasurable() {
        // An unmeasurable target must never block writes — a `df` hiccup
        // can't be allowed to halt every backup.
        let capacity = capacity_with("/nonexistent-capacity-test-dir", Vec::new()).await;
        let verdict = capacity.reserve(ReserveRequest::new("unit test write")).await;
        assert_eq!(verdict, Verdict::Allow);
    }

    #[tokio::test]
    async fn pressure_starts_healthy() {
        let capacity = capacity_with("/nonexistent-capacity-test-dir", Vec::new()).await;
        assert_eq!(capacity.current_pressure(), Pressure::Healthy);
    }
}
