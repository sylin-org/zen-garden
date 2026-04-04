//! BackgroundTask: storage-beacon-subscriber (ARCH-0015, STORAGE-0013)
//!
//! Long-running task that reacts to StorageChanged domain events by
//! broadcasting storage beacons with debounce.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageBeaconSubscriberTask;

impl BackgroundTask for StorageBeaconSubscriberTask {
    fn name(&self) -> &'static str {
        "storage-beacon-subscriber"
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
            crate::tasks::storage_orchestration::storage_beacon_subscriber(ctx.state, ctx.token)
                .await;
            TaskOutcome::Cancelled
        })
    }
}
