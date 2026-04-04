//! BackgroundTask: s3-listener-lifecycle (ARCH-0015, STORAGE-0016)
//!
//! Long-running task that arms/disarms per-storage S3 ports in response
//! to storage change events.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct S3ListenerLifecycleTask;

impl BackgroundTask for S3ListenerLifecycleTask {
    fn name(&self) -> &'static str {
        "s3-listener-lifecycle"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["volume-monitor"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }

            let mut rx = ctx.state.subscribe_storage_changed();
            tracing::info!("S3 listener lifecycle task started (STORAGE-0016)");

            // Initial arm for all existing primaries
            crate::tasks::storage_tasks::arm_s3_for_all_primaries(&ctx.state).await;
            ctx.ready.signal();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("S3 listener lifecycle shutting down");
                        break;
                    }
                    result = rx.recv() => match result {
                        Ok(garden_common::storage::StorageChanged::Added { .. })
                        | Ok(garden_common::storage::StorageChanged::Connected { .. })
                        | Ok(garden_common::storage::StorageChanged::Reclassified) => {
                            crate::tasks::storage_tasks::arm_s3_for_all_primaries(&ctx.state).await;
                        }
                        Ok(garden_common::storage::StorageChanged::Removed { .. })
                        | Ok(garden_common::storage::StorageChanged::Released { .. }) => {
                            crate::tasks::storage_tasks::reconcile_s3_listeners(&ctx.state).await;
                        }
                        Ok(garden_common::storage::StorageChanged::RoleChanged { .. }) => {
                            crate::tasks::storage_tasks::reconcile_s3_listeners(&ctx.state).await;
                            crate::tasks::storage_tasks::arm_s3_for_all_primaries(&ctx.state).await;
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            crate::tasks::storage_tasks::reconcile_s3_listeners(&ctx.state).await;
                            crate::tasks::storage_tasks::arm_s3_for_all_primaries(&ctx.state).await;
                        }
                    },
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
