//! BackgroundTask: registry-maintenance (ARCH-0015)
//!
//! Periodically reaps expired gateway entries from the tool registry
//! and publishes deltas for removed entries.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct RegistryMaintenanceTask;

impl BackgroundTask for RegistryMaintenanceTask {
    fn name(&self) -> &'static str {
        "registry-maintenance"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
            interval.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Registry maintenance shutting down");
                        break;
                    }
                }

                let events = ctx.state.tool.reap_expired_gateways().await;
                if !events.is_empty() {
                    let count = events.iter().filter(|e| e.as_delta().is_some()).count();
                    crate::domain::tool::projection::publish_events_for_state(&ctx.state, &events)
                        .await;
                    tracing::debug!(count, "Reaped expired gateway entries");
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
