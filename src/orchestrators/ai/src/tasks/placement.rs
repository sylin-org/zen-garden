//! Placement task — demand-weighted model→stone assignment.
//!
//! Periodically computes the ideal placement plan from demand distribution
//! and VRAM constraints. When the plan changes, emits an event for the
//! resource_sync task to act on.
//!
//! Generalized from ollama-orchestrator tasks/placement.rs.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::placement;

const PLACEMENT_INTERVAL: Duration = Duration::from_secs(60);

/// Background task: periodic placement recomputation.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(PLACEMENT_INTERVAL) => {}
        }

        let demand_shares = {
            let metrics = state.metrics.read().await;
            metrics.demand_shares(3600)
        };

        if demand_shares.is_empty() {
            continue;
        }

        let instances = state.instances.read().await.clone();
        let models = state.models.read().await.clone();

        let new_plan = placement::compute_placement(&demand_shares, &instances, &models);

        let changed = {
            let current = state.placement.read().await;
            !placement::plans_equivalent(&current, &new_plan)
        };

        if changed {
            tracing::info!(
                models = new_plan.assignments.len(),
                "placement: new plan computed"
            );
            *state.placement.write().await = new_plan;
            state.emit_event("placement.updated", "{}").await;
        }
    }

    tracing::info!("placement task shutting down");
}
