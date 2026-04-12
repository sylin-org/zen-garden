//! Offering reconciliation — auto-rebuilds missing managed containers (OFFER-0008).
//!
//! Extracted from the health monitor to keep concerns separated:
//! - Health monitor detects missing containers and tracks `confirmed_missing`
//! - This module manages backoff, in-flight guards, bounded concurrency, and
//!   dispatches reconciliation jobs
//!
//! ## Usage
//!
//! The health monitor creates a `ReconciliationCoordinator` before the loop
//! and calls `process_missing_offerings()` each cycle with the set of
//! offerings whose containers are confirmed missing.

use crate::AppState;
use crate::domain::events::OfferingEvent;
use garden_common::console::{self, EventCategory, EventStatus};
use garden_common::{OfferingStatus, ServiceHealthStatus};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

// ============================================================================
// ReconciliationTracker — per-offering backoff state
// ============================================================================

/// Per-offering reconciliation backoff state (in-memory only).
///
/// Backoff schedule: 30s, 60s, 120s, 240s, 480s (5 attempts max).
/// After exhaustion the offering is marked `Degraded`.
/// A daemon restart resets all trackers (intentional — the restart is the
/// operator's signal to retry).
pub(crate) struct ReconciliationTracker {
    pub(crate) attempts: u8,
    next_eligible: tokio::time::Instant,
}

impl ReconciliationTracker {
    pub(crate) fn new() -> Self {
        Self {
            attempts: 0,
            next_eligible: tokio::time::Instant::now(),
        }
    }

    /// Record a failed attempt and compute next eligible time with exponential backoff.
    pub(crate) fn record_failure(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
        let backoff_secs = 30u64 * 2u64.pow((self.attempts - 1).min(4) as u32);
        self.next_eligible =
            tokio::time::Instant::now() + tokio::time::Duration::from_secs(backoff_secs);
    }

    /// Record a successful reconciliation — resets the tracker.
    pub(crate) fn record_success(&mut self) {
        self.attempts = 0;
        self.next_eligible = tokio::time::Instant::now();
    }

    pub(crate) fn is_eligible(&self) -> bool {
        self.attempts < 5 && tokio::time::Instant::now() >= self.next_eligible
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.attempts >= 5
    }
}

// ============================================================================
// ReconciliationCoordinator — owns backoff, in-flight, and semaphore
// ============================================================================

/// Coordinates auto-reconciliation of missing managed containers.
///
/// Owns the per-offering backoff trackers, the in-flight guard set, and
/// the bounded concurrency semaphore. Created once at health monitor startup
/// and called each cycle.
pub(crate) struct ReconciliationCoordinator {
    backoff: Arc<tokio::sync::Mutex<HashMap<String, ReconciliationTracker>>>,
    in_flight: Arc<tokio::sync::Mutex<HashSet<String>>>,
    semaphore: Arc<Semaphore>,
}

impl ReconciliationCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            backoff: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            in_flight: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    /// Check if an offering name is currently in-flight (for TOPO-0002 guard).
    pub(crate) async fn is_in_flight(&self, name: &str) -> bool {
        self.in_flight.lock().await.contains(name)
    }

    /// Advisory check for log-level decisions. Stale reads are acceptable.
    pub(crate) async fn is_tracked_or_in_flight(&self, name: &str) -> bool {
        let bt = self.backoff.lock().await;
        let ifl = self.in_flight.lock().await;
        bt.contains_key(name) || ifl.contains(name)
    }

    /// Prune backoff entries for offerings that no longer exist in the registry.
    pub(crate) async fn prune_stale(&self, live_names: &HashSet<String>) {
        self.backoff
            .lock()
            .await
            .retain(|name, _| live_names.contains(name));
    }

    /// Process a set of confirmed-missing managed offerings.
    ///
    /// For each offering:
    /// 1. Check backoff eligibility
    /// 2. Mark in-flight + set status to Installing
    /// 3. Spawn a bounded reconciliation task
    ///
    /// Returns whether any state changes were made (for chirp batching).
    pub(crate) async fn process_missing_offerings(
        &self,
        state: &AppState,
        token: &CancellationToken,
        confirmed_missing: &HashSet<String>,
    ) -> bool {
        let mut state_changed = false;

        // Snapshot offerings (drop read guard before acquiring in_flight)
        let candidates_raw: Vec<(String, String, OfferingStatus)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| {
                    o.is_managed()
                        && o.status != OfferingStatus::Installing
                        && o.status != OfferingStatus::Cordoned
                })
                .map(|o| (o.offering_id.clone(), o.name.to_string(), o.status))
                .collect()
        };

        // Filter to confirmed-missing and not in-flight (separate lock scope)
        let reconcile_candidates: Vec<(String, String, OfferingStatus)> = {
            let currently_in_flight = self.in_flight.lock().await;
            candidates_raw
                .into_iter()
                .filter(|(_, name, _)| {
                    confirmed_missing.contains(name) && !currently_in_flight.contains(name)
                })
                .collect()
        };

        for (offering_id, name, pre_status) in reconcile_candidates {
            // Check backoff eligibility
            {
                let mut bt = self.backoff.lock().await;
                let tracker = bt
                    .entry(name.clone())
                    .or_insert_with(ReconciliationTracker::new);
                if tracker.is_exhausted() {
                    if pre_status != OfferingStatus::Degraded {
                        state.console.emit(console::ConsoleEvent::new(
                            EventCategory::Health,
                            EventStatus::Degraded,
                            format!("{} — reconciliation exhausted (5/5)", name),
                        ));
                        tracing::warn!(
                            offering = %name,
                            attempts = tracker.attempts,
                            "Reconciliation exhausted, marking as degraded"
                        );
                        state
                            .offerings
                            .update(&offering_id, |o| {
                                o.status = OfferingStatus::Degraded;
                                true
                            })
                            .await;
                        state_changed = true;
                    }
                    continue;
                }
                if !tracker.is_eligible() {
                    continue;
                }
            }

            // Mark in-flight and set status to Installing (gate for next cycle)
            self.in_flight.lock().await.insert(name.clone());
            state
                .offerings
                .update(&offering_id, |o| {
                    o.status = OfferingStatus::Installing;
                    true
                })
                .await;

            // Desired state: if the offering was Stopped before, don't auto-start.
            let target_status = if pre_status == OfferingStatus::Stopped {
                OfferingStatus::Stopped
            } else {
                OfferingStatus::Running
            };

            // Spawn bounded reconciliation task
            let state = state.clone();
            let token = token.clone();
            let semaphore = self.semaphore.clone();
            let in_flight = self.in_flight.clone();
            let backoff = self.backoff.clone();

            tokio::spawn(async move {
                // Acquire semaphore permit (bounded concurrency).
                // acquire() only returns Err if close() is called, which we never do.
                let _permit = match semaphore.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        in_flight.lock().await.remove(&name);
                        return;
                    }
                };

                if token.is_cancelled() {
                    in_flight.lock().await.remove(&name);
                    return;
                }

                // Emit console event for tty1 visibility (OFFER-0008)
                {
                    let attempts = backoff
                        .lock()
                        .await
                        .get(&name)
                        .map(|t| t.attempts + 1)
                        .unwrap_or(1);
                    state.console.emit(console::ConsoleEvent::new(
                        EventCategory::Services,
                        EventStatus::Reconciling,
                        format!("{} (attempt {}/5)", name, attempts),
                    ));
                }
                tracing::info!(
                    offering = %name,
                    target_status = %target_status,
                    "Starting auto-reconciliation"
                );

                match crate::domain::services_internal::reconcile_offering(&state, &name).await {
                    Ok(result) => {
                        result.apply_port_updates(&state, &offering_id).await;

                        if target_status == OfferingStatus::Stopped
                            && let Err(e) = state.platform.container.stop_service(&name, None).await
                        {
                            tracing::warn!(
                                offering = %name,
                                error = ?e,
                                "Failed to stop reconciled container (was stopped before wipe)"
                            );
                        }

                        // auto_chirp=true so the garden learns the offering is back
                        state
                            .offerings
                            .update(&offering_id, |o| {
                                o.status = target_status;
                                o.health = if target_status == OfferingStatus::Running {
                                    ServiceHealthStatus::Healthy
                                } else {
                                    ServiceHealthStatus::Offline
                                };
                                true
                            })
                            .await;

                        // Emit domain event for SSE consumers, companions, orchestrators
                        state.event_bus.emit(OfferingEvent::started(
                            &offering_id,
                            &name,
                            state.stone_name(),
                        ));

                        if let Some(tracker) = backoff.lock().await.get_mut(&name) {
                            tracker.record_success();
                        }

                        // Console event: reconciled (goes to tty1)
                        let port_note = if result.ports_changed {
                            "ports remapped"
                        } else {
                            "ports preserved"
                        };
                        state.console.emit(console::ConsoleEvent::new(
                            EventCategory::Services,
                            EventStatus::Reconciled,
                            format!("{} — {}", name, port_note),
                        ));

                        tracing::info!(
                            offering = %name,
                            status = %target_status,
                            "Auto-reconciliation succeeded"
                        );
                    }
                    Err(e) => {
                        // Console event: failure (goes to tty1)
                        state.console.emit(console::ConsoleEvent::new(
                            EventCategory::Services,
                            EventStatus::ReconcileError,
                            format!("{} — {:#}", name, e),
                        ));

                        tracing::warn!(
                            offering = %name,
                            error = ?e,
                            "Auto-reconciliation failed"
                        );

                        backoff
                            .lock()
                            .await
                            .entry(name.clone())
                            .or_insert_with(ReconciliationTracker::new)
                            .record_failure();

                        state
                            .offerings
                            .update(&offering_id, |o| {
                                o.status = OfferingStatus::Stopped;
                                o.health = ServiceHealthStatus::Offline;
                                true
                            })
                            .await;
                    }
                }

                in_flight.lock().await.remove(&name);
            });
        }

        state_changed
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_starts_eligible() {
        let tracker = ReconciliationTracker::new();
        assert_eq!(tracker.attempts, 0);
        assert!(tracker.is_eligible());
        assert!(!tracker.is_exhausted());
    }

    #[test]
    fn tracker_backoff_progression() {
        let mut tracker = ReconciliationTracker::new();
        for expected_attempt in 1..=5u8 {
            tracker.record_failure();
            assert_eq!(tracker.attempts, expected_attempt);
        }
    }

    #[test]
    fn tracker_exhausted_after_five_failures() {
        let mut tracker = ReconciliationTracker::new();
        for _ in 0..4 {
            tracker.record_failure();
            assert!(!tracker.is_exhausted());
        }
        tracker.record_failure();
        assert!(tracker.is_exhausted());
        assert!(!tracker.is_eligible());
    }

    #[test]
    fn tracker_not_eligible_during_backoff() {
        let mut tracker = ReconciliationTracker::new();
        tracker.record_failure();
        assert!(!tracker.is_eligible());
    }

    #[test]
    fn tracker_success_resets() {
        let mut tracker = ReconciliationTracker::new();
        tracker.record_failure();
        tracker.record_failure();
        tracker.record_failure();
        assert_eq!(tracker.attempts, 3);

        tracker.record_success();
        assert_eq!(tracker.attempts, 0);
        assert!(tracker.is_eligible());
        assert!(!tracker.is_exhausted());
    }

    #[test]
    fn tracker_attempts_saturate() {
        let mut tracker = ReconciliationTracker::new();
        for _ in 0..255 {
            tracker.record_failure();
        }
        assert!(tracker.is_exhausted());
        assert_eq!(tracker.attempts, 255);
    }

    #[test]
    fn tracker_backoff_schedule_seconds() {
        let expected = [30u64, 60, 120, 240, 480];
        let mut tracker = ReconciliationTracker::new();
        let base = tokio::time::Instant::now();

        for (i, &expected_secs) in expected.iter().enumerate() {
            tracker.record_failure();
            let elapsed = tracker.next_eligible - base;
            let actual_secs = elapsed.as_secs();
            assert!(
                actual_secs >= expected_secs.saturating_sub(1) && actual_secs <= expected_secs + 1,
                "attempt {}: expected ~{}s, got {}s",
                i + 1,
                expected_secs,
                actual_secs,
            );
        }
    }
}
