//! Placement reconciler: computes demand-weighted model→stone assignments
//! and pre-warms models on their target stones.
//!
//! Runs every 60 seconds.  Only acts on stable plans (same result for
//! 2 consecutive computations) to prevent thrashing from bursty traffic.

use crate::app_state::AppState;
use crate::domain::placement;
use crate::infra::ollama_client::OllamaClient;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Placement recomputation interval.
const PLACEMENT_INTERVAL: Duration = Duration::from_secs(60);

/// Demand window for computing model request shares.
const DEMAND_WINDOW_SECS: u64 = 300; // 5 minutes

/// Run the placement reconciler loop.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
    // Wait for discovery and initial traffic to build up.
    tokio::time::sleep(Duration::from_secs(60)).await;

    let mut previous_plan = crate::domain::types::PlacementPlan::default();
    let mut consecutive_stable = 0u32;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(PLACEMENT_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        // ── Compute new placement plan ───────────────────────────
        let (demand_shares, instances_snap, models_snap) = {
            let metrics = state.metrics.read().await;
            let instances = state.instances.read().await;
            let models = state.models.read().await;
            (
                metrics.demand_shares(DEMAND_WINDOW_SECS),
                instances.clone(),
                models.clone(),
            )
        };

        if demand_shares.is_empty() {
            continue;
        }

        let mut new_plan =
            placement::compute_placement(&demand_shares, &instances_snap, &models_snap);

        // ── Hysteresis: check if plan is stable ──────────────────
        if placement::plans_equivalent(&new_plan, &previous_plan) {
            consecutive_stable += 1;
            new_plan.stable = consecutive_stable >= 2;
        } else {
            consecutive_stable = 0;
            new_plan.stable = false;
        }

        tracing::debug!(
            models = ?new_plan.assignments.keys().collect::<Vec<_>>(),
            stable = new_plan.stable,
            consecutive = consecutive_stable,
            "placement plan computed"
        );

        // ── Publish plan ─────────────────────────────────────────
        previous_plan = new_plan.clone();
        {
            let mut plan = state.placement.write().await;
            *plan = new_plan.clone();
        }

        // ── Reconcile: pre-warm if plan is stable ────────────────
        if new_plan.stable {
            reconcile(&state, &client, &new_plan, &instances_snap).await;
        }
    }
}

/// Pre-warm models on their assigned stones if not already loaded.
///
/// Only warms models that are available (on disk) but not loaded (in VRAM).
/// If a model isn't even available on the target stone, the model_sync task
/// will handle pulling it first.
async fn reconcile(
    state: &AppState,
    client: &OllamaClient,
    plan: &crate::domain::types::PlacementPlan,
    instances: &std::collections::HashMap<String, crate::domain::types::OllamaInstance>,
) {
    for (model, target_endpoints) in &plan.assignments {
        for endpoint in target_endpoints {
            // Check if model is already loaded on this stone
            let already_loaded = instances
                .get(endpoint)
                .map(|i| i.models_loaded.iter().any(|l| l.name == *model))
                .unwrap_or(false);

            if already_loaded {
                continue;
            }

            // Check if model is available (on disk) on this stone
            let available = instances
                .get(endpoint)
                .map(|i| i.models_available.iter().any(|m| m == model))
                .unwrap_or(false);

            if !available {
                tracing::debug!(
                    model = %model,
                    endpoint = %endpoint,
                    "placement: model not available on stone, skipping (model_sync will handle)"
                );
                continue;
            }

            let stone_name = instances
                .get(endpoint)
                .map(|i| i.stone_name.as_str())
                .unwrap_or("unknown");

            tracing::info!(
                model = %model,
                stone = %stone_name,
                endpoint = %endpoint,
                "placement: pre-warming model"
            );

            match client.load_model(endpoint, model).await {
                Ok(()) => {
                    tracing::info!(model = %model, stone = %stone_name, "placement: model pre-warmed");
                    state
                        .emit_event(
                            "placement.warmed",
                            &serde_json::json!({
                                "model": model,
                                "stone": stone_name,
                            })
                            .to_string(),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::warn!(
                        model = %model,
                        stone = %stone_name,
                        error = %e,
                        "placement: pre-warm failed"
                    );
                }
            }
        }
    }
}
