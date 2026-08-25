//! BackgroundTask: periodic-announcer (ARCH-0015)
//!
//! Long-running task that broadcasts stone presence at 30s intervals.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct PeriodicAnnouncerTask;

impl BackgroundTask for PeriodicAnnouncerTask {
    fn name(&self) -> &'static str {
        "periodic-announcer"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();
            crate::tasks::announcer::periodic_announcer_task(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
