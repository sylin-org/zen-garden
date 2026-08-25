//! BackgroundTask: storage-orchestration (ARCH-0015, STORAGE-0006)
//!
//! Long-running task that assigns Primary/Replica roles for replicated
//! seed banks.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageOrchestrationTask;

impl BackgroundTask for StorageOrchestrationTask {
    fn name(&self) -> &'static str {
        "storage-orchestration"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["volume-monitor"]
    }

    fn run(
        self: Box<Self>,
        mut ctx: TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }
            ctx.ready.signal();

            if let Err(e) = crate::tasks::storage_orchestration::storage_orchestration_task(
                ctx.state, ctx.token,
            )
            .await
            {
                tracing::error!(error = ?e, "Seed bank orchestration task failed");
                return TaskOutcome::Failed {
                    error: format!("{e:#}"),
                };
            }
            TaskOutcome::Cancelled
        })
    }
}
