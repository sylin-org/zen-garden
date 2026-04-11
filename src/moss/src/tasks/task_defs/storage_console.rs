//! BackgroundTask: storage-console (ARCH-0015)
//!
//! Long-running task that renders connected/released ribbons to the
//! physical console in response to storage change events.

use std::future::Future;
use std::pin::Pin;

use garden_common::storage::StorageChanged;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageConsoleTask;

impl BackgroundTask for StorageConsoleTask {
    fn name(&self) -> &'static str {
        "storage-console"
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

            let runtime = ctx.state.platform.runtime.clone();
            let mut rx = ctx.state.subscribe_storage_changed();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(StorageChanged::Sensed { .. }) => {}
                        Ok(StorageChanged::Connected { name, roles, used_bytes, capacity_bytes }) => {
                            runtime.print_storage_connected(&name, &roles, used_bytes, capacity_bytes);
                        }
                        Ok(StorageChanged::Released { name }) => {
                            runtime.print_storage_released(&name);
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    },
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
