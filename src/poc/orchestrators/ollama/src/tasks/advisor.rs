//! Topology advisor task: recomputes GPU placement + parallelism
//! recommendations whenever topology changes or on a periodic timer.
//!
//! Two triggers:
//! 1. **Reactive** — listens for `registry.updated` dashboard events
//!    (fired by `upsert_instance`, `remove_instance`, model changes).
//!    Debounced to avoid thrashing during rapid discovery.
//! 2. **Periodic** — every `ADVISOR_INTERVAL` as a safety net, so the
//!    dashboard always has a reasonably fresh recommendation.

use crate::app_state::AppState;
use crate::domain::advisor::{self, DemandContext};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Periodic re-evaluation interval.
const ADVISOR_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// Debounce window after a topology event before recomputing.
const DEBOUNCE: Duration = Duration::from_secs(5);

/// Initial delay — wait for discovery to populate instances + models.
const STARTUP_DELAY: Duration = Duration::from_secs(15);

/// Run the advisor loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Give discovery time to populate the registry.
    tokio::select! {
        _ = tokio::time::sleep(STARTUP_DELAY) => {}
        _ = shutdown.cancelled() => return,
    }

    // Compute the initial T=0 advice.
    recompute(&state, "initial").await;

    // Subscribe to dashboard events for topology changes.
    let mut events = state.dashboard_tx.subscribe();

    loop {
        tokio::select! {
            // ── Reactive: topology changed ───────────────────────
            result = events.recv() => {
                match result {
                    Ok(evt) if is_topology_event(&evt.event_type) => {
                        // Debounce: wait a few seconds for the burst to settle
                        // (discovery often fires multiple events in quick succession).
                        tokio::select! {
                            _ = tokio::time::sleep(DEBOUNCE) => {}
                            _ = shutdown.cancelled() => return,
                        }
                        // Drain any extra events that arrived during debounce
                        while events.try_recv().is_ok() {}
                        recompute(&state, "topology_change").await;
                    }
                    Ok(_) => {} // Ignore non-topology events
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!(skipped = n, "advisor event lag — recomputing");
                        recompute(&state, "event_lag").await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }

            // ── Periodic: safety-net re-evaluation ───────────────
            _ = tokio::time::sleep(ADVISOR_INTERVAL) => {
                recompute(&state, "periodic").await;
            }

            // ── Shutdown ─────────────────────────────────────────
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Events that indicate the topology may have changed.
fn is_topology_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "registry.updated" | "models.updated" | "tiers.updated"
    )
}

/// Snapshot current state, run the advisor algorithm, and publish results.
async fn recompute(state: &AppState, trigger: &str) {
    let (gpu_slots, model_slots, demand_ctx) = {
        let instances = state.instances.read().await;
        let models = state.models.read().await;
        let demand_ledger = state.demand_ledger.read().await;
        let benchmark = state.benchmark_run.read().await;
        let gpu_matrix = &benchmark.gpu_matrix;

        let gs = advisor::gpu_slots_from_instances(
            &instances,
            Some(&demand_ledger),
            Some(gpu_matrix),
        );
        let ms = advisor::model_slots_projected(&models);

        // Build demand context from the ledger
        let now = Instant::now();
        let ctx = if demand_ledger.total_requests > 0 {
            Some(DemandContext {
                capability_distribution: demand_ledger.capability_distribution(now),
                model_distribution: demand_ledger.model_distribution(now),
                confidence: demand_ledger.confidence(),
                capability_rates: demand_ledger.capability_rates(now),
            })
        } else {
            None
        };

        tracing::debug!(
            trigger,
            gpu_slots = gs.len(),
            model_slots = ms.len(),
            instances_total = instances.len(),
            models_total = models.len(),
            demand_confidence = ctx.as_ref().map(|d| d.confidence).unwrap_or(0.0),
            "advisor: input snapshot"
        );
        (gs, ms, ctx)
    };

    let mut advice = advisor::advise_topology(&gpu_slots, &model_slots, demand_ctx.as_ref());
    advice.computed_at = Some(chrono::Utc::now().to_rfc3339());
    advice.trigger = trigger.to_string();

    let has_recs = advice.has_recommendations;
    let gpu_count = advice.gpus.len();
    let model_count: usize = advice.gpus.iter().map(|g| g.models.len()).sum();

    {
        let mut advisor_state = state.advisor.write().await;
        *advisor_state = advice;
    }

    if has_recs {
        tracing::info!(
            trigger,
            gpus = gpu_count,
            models = model_count,
            "advisor: topology recommendations available"
        );
    } else {
        tracing::debug!(
            trigger,
            gpus = gpu_count,
            models = model_count,
            "advisor: topology looks good"
        );
    }
}
