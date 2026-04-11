//! BackgroundTask: auto-adoption (ARCH-0015)
//!
//! Long-running task that periodically scans for unmanaged containers
//! and adopts them if they match known offering templates.
//!
//! Pattern C: carries `AdoptionConfig` constructed at registration time.

use std::future::Future;
use std::pin::Pin;

use crate::infra::AdoptionConfig;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct AutoAdoptionTask {
    pub config: AdoptionConfig,
}

impl BackgroundTask for AutoAdoptionTask {
    fn name(&self) -> &'static str {
        "auto-adoption"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["docker-events"]
    }

    fn run(
        self: Box<Self>,
        mut ctx: TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        let config = self.config;

        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }
            ctx.ready.signal();
            crate::tasks::auto_adoption::auto_adoption_task(ctx.state, config, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
