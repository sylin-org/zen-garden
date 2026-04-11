//! BackgroundTask: metrics-collector (ARCH-0015)
//!
//! Long-running task that feeds presence protocol and health monitors.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct MetricsCollectorTask;

impl BackgroundTask for MetricsCollectorTask {
    fn name(&self) -> &'static str {
        "metrics-collector"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();
            crate::tasks::metrics_collector::run_metrics_collector(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
