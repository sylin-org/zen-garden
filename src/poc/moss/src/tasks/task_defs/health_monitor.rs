//! BackgroundTask: health-monitor (ARCH-0015)
//!
//! Long-running task that periodically checks container health, updates
//! offering statuses, adopts discovered containers, and persists changes.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct HealthMonitorTask;

impl BackgroundTask for HealthMonitorTask {
    fn name(&self) -> &'static str {
        "health-monitor"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["docker-events"]
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
            crate::tasks::health_monitor::health_monitor_task(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
