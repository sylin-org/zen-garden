//! BackgroundTask: registry-loader (ARCH-0015)
//!
//! One-shot task that reconciles managed offerings against Docker state,
//! coalesces duplicates, backfills guidance and scheduled tasks, and
//! adopts existing containers.

use std::future::Future;
use std::pin::Pin;

use garden_common::ServiceHealthStatus;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct RegistryLoaderTask;

impl BackgroundTask for RegistryLoaderTask {
    fn name(&self) -> &'static str {
        "registry-loader"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["docker-events"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }

            let state = &ctx.state;

            // Reconcile existing offerings: if the container no longer exists, mark it offline
            let managed_snapshot: Vec<(String, String)> = {
                let offerings = state.offerings.read().await;
                offerings
                    .iter()
                    .filter(|o| o.is_managed())
                    .map(|o| (o.offering_id.clone(), o.name.to_string()))
                    .collect()
            };
            let mut any_changed = false;
            for (offering_id, name) in managed_snapshot {
                if !state
                    .platform
                    .docker
                    .zen_container_exists(&name)
                    .await
                    .unwrap_or(false)
                {
                    state
                        .update_offering(&offering_id, false, |o| {
                            o.status = garden_common::OfferingStatus::Stopped;
                            o.health = ServiceHealthStatus::Offline;
                            true
                        })
                        .await;
                    any_changed = true;
                }
            }
            if any_changed {
                state.sync_self_services(true).await;
            }

            let coalesced = state.coalesce_duplicate_offerings().await;
            if coalesced > 0 {
                tracing::info!(coalesced, "Startup: removed duplicate offerings by FQN");
            }

            let backfilled = crate::tasks::backfill_missing_guidance(state).await;
            if backfilled > 0 {
                tracing::info!(
                    count = backfilled,
                    "Backfilled guidance for existing services"
                );
            }

            let tasks_backfilled =
                crate::tasks::task_scheduler::backfill_missing_tasks(state).await;
            if tasks_backfilled > 0 {
                tracing::info!(
                    count = tasks_backfilled,
                    "Backfilled scheduled tasks for existing services"
                );
            }

            crate::domain::adopt_existing_containers(state).await;

            ctx.ready.signal();
            TaskOutcome::Completed
        })
    }
}
