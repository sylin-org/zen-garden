//! BackgroundTask: resources-collector (ARCH-0015)
//!
//! Long-running task that collects hardware resource snapshots (CPU,
//! memory, disk, network, GPU) and feeds the presence protocol and
//! health monitors. Renamed from `metrics-collector` in ARCH-0018
//! Book I Chapter 2 — "metrics" now exclusively means software
//! observability (see `domain::metrics`).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct ResourcesCollectorTask;

impl BackgroundTask for ResourcesCollectorTask {
    fn name(&self) -> &'static str {
        "resources-collector"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();
            crate::tasks::resources_collector::run_resources_collector(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
