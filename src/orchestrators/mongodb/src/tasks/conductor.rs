//! Conductor — single event loop for replica set orchestration.
//!
//! Replaces the former `bootstrap` and `health_monitor` tasks with a unified
//! decision-maker that delegates to `ReplicaManager`.
//!
//! Two signal sources:
//! - **Reactive** (`Arc<Notify>`): discovery or API mutated instance registry
//!   → reconcile all FQNs immediately
//! - **Periodic** (15s timer): health check → reconcile only if broken

use crate::app_state::AppState;
use crate::replica_manager::ReplicaManager;
use std::time::Duration;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

/// How often to run periodic health checks (seconds).
const HEALTH_INTERVAL_SECS: u64 = 15;

/// Run the conductor task.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Wait for initial discovery to populate instances
    tokio::select! {
        _ = shutdown.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_secs(15)) => {}
    }

    let rm = ReplicaManager::new(state.clone());
    let mut timer = interval(Duration::from_secs(HEALTH_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("conductor shutting down");
                return;
            }

            _ = state.conductor_notify.notified() => {
                // Reactive: registry changed → reconcile all FQNs
                tracing::debug!("conductor woke up: registry change signal");
                for fqn in state.distinct_fqns().await {
                    let instances = state.instances_for_fqn(&fqn).await;
                    rm.reconcile(&fqn, &instances).await;
                }
            }

            _ = timer.tick() => {
                // Periodic: health check → reconcile if broken
                for fqn in state.distinct_fqns().await {
                    let instances = state.instances_for_fqn(&fqn).await;
                    let result = rm.check(&fqn, &instances).await;
                    if result.needs_reconciliation {
                        rm.reconcile(&fqn, &instances).await;
                    }
                }
            }
        }
    }
}
