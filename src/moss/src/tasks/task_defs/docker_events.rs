//! BackgroundTask: docker-events (ARCH-0015)
//!
//! Long-running task that subscribes to Docker daemon events and updates
//! offering/service state accordingly.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct DockerEventsTask;

impl BackgroundTask for DockerEventsTask {
    fn name(&self) -> &'static str {
        "docker-events"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            // Signal ready immediately — downstream tasks need the Docker client,
            // not the event stream itself.
            ctx.ready.signal();
            crate::tasks::docker_events::docker_events_task(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
