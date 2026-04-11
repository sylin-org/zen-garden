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

                let reaped = ctx
                    .state
                    .tool
                    .registry
                    .write()
                    .await
                    .reap_expired_gateways();
                if !reaped.is_empty() {
                    let count = reaped.len();
                    ctx.state.publish_tool_deltas(reaped, true).await;
                    tracing::debug!(count, "Reaped expired gateway entries");
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
