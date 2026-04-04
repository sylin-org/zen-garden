//! BackgroundTask: storage-lifecycle (ARCH-0015, STORAGE-0011)
//!
//! Long-running 10s interval task that handles auto-mount, health ticks,
//! tools projection refresh, and storage beacon broadcast.

use std::future::Future;
use std::pin::Pin;

use crate::infra::storage::OsPlatform;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageLifecycleTask;

impl BackgroundTask for StorageLifecycleTask {
    fn name(&self) -> &'static str {
        "storage-lifecycle"
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

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.tick().await;
            tracing::info!("Storage lifecycle task started (10s interval)");

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Storage lifecycle shutting down");
                        break;
                    }
                }

                let mounted =
                    crate::domain::storage::auto_mount_unmounted(&OsPlatform).await;
                if mounted > 0 {
                    ctx.state
                        .emit_storage_changed(
                            garden_common::storage::StorageChanged::Reclassified,
                        )
                        .await;
                }

                crate::domain::storage::observe_all(
                    &ctx.state.current.storage.volumes,
                    &OsPlatform,
                )
                .await;

                ctx.state.refresh_local_tools_projection().await;

                let has_managed = {
                    let map = ctx.state.current.storage.volumes.read().await;
                    map.values().any(|v| v.is_managed())
                };
                if has_managed {
                    ctx.state.broadcast_storage_beacon().await;
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
