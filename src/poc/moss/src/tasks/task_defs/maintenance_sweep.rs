//! BackgroundTask: maintenance-sweep (ARCH-0015)
//!
//! Hourly caretaking sweep that runs all domain sweepers and persists results.
//! Waits 5 minutes after boot before the first sweep.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct MaintenanceSweepTask;

impl BackgroundTask for MaintenanceSweepTask {
    fn name(&self) -> &'static str {
        "maintenance-sweep"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            // Wait 5 minutes after boot before first sweep (or exit early on shutdown)
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(300)) => {}
                _ = ctx.token.cancelled() => {
                    tracing::debug!("Maintenance sweep cancelled during startup delay");
                    return TaskOutcome::Cancelled;
                }
            }

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Maintenance sweep shutting down");
                        break;
                    }
                }

                let task_store = crate::infra::TaskStore::new();
                let run = crate::domain::maintenance::run_sweep(&ctx.state, &task_store).await;
                tracing::info!(
                    status = ?run.overall_status,
                    duration_ms = run.duration_ms,
                    domains = run.reports.len(),
                    "Maintenance sweep complete"
                );

                if let Err(e) = crate::infra::maintenance_store::save_sweep_run(&run).await {
                    tracing::warn!(error = ?e, "Failed to save sweep report");
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
