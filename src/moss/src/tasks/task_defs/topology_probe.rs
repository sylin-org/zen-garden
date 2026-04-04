//! BackgroundTask: topology-probe (ARCH-0015)
//!
//! Delegates to the existing `restore_or_probe` function (Tier 2 hardware topology, ARCH-0014).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct TopologyProbeTask;

impl BackgroundTask for TopologyProbeTask {
    fn name(&self) -> &'static str {
        "topology-probe"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            let current = ctx.state.current.clone();
            let console = ctx.state.console.clone();

            crate::tasks::topology_probe::restore_or_probe(current, console).await;

            ctx.ready.signal();
            TaskOutcome::Completed
        })
    }
}
