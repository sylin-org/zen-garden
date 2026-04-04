//! BackgroundTask: storage-replication (ARCH-0015, STORAGE-0006 Phase 4e)
//!
//! Long-running task that syncs Dormant seed banks from their Primaries.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageReplicationTask;

impl BackgroundTask for StorageReplicationTask {
    fn name(&self) -> &'static str {
        "storage-replication"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["volume-monitor"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }
            ctx.ready.signal();

            if let Err(e) =
                crate::tasks::storage_replication::storage_replication_task(ctx.state, ctx.token)
                    .await
            {
                tracing::error!(error = ?e, "Seed bank replication task failed");
                return TaskOutcome::Failed {
                    error: format!("{e:#}"),
                };
            }
            TaskOutcome::Cancelled
        })
    }
}
