//! BackgroundTask: initial-service-sync (ARCH-0015)
//!
//! One-shot task that synchronises the local service list after registry
//! loading is complete. Replaces the old 2-second sleep hack — the
//! dependency gate ensures registry-loader has finished first.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct InitialServiceSyncTask;

impl BackgroundTask for InitialServiceSyncTask {
    fn name(&self) -> &'static str {
        "initial-service-sync"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["registry-loader"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }

            ctx.state.sync_self_services(true).await;

            ctx.ready.signal();
            TaskOutcome::Completed
        })
    }
}
