//! BackgroundTask: topology-maintenance (ARCH-0015)
//!
//! Periodically marks stale stones offline, evicts old entries, and
//! flushes dirty topology cache to disk (TOPO-0002).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct TopologyMaintenanceTask;

impl BackgroundTask for TopologyMaintenanceTask {
    fn name(&self) -> &'static str {
        "topology-maintenance"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            interval.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Topology maintenance shutting down");
                        break;
                    }
                }

                let self_entry =
                    crate::domain::topology::composition::build_self_entry(&ctx.state).await;
                let (marked, evicted) = ctx.state.topology.maintain(&self_entry).await;

                if marked > 0 || evicted > 0 {
                    tracing::debug!(
                        marked_offline = marked,
                        evicted = evicted,
                        "Topology maintenance complete"
                    );
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
