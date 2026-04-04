//! BackgroundTask: election-listener (ARCH-0015)
//!
//! Runs the election service p2p listener for distributed leader election.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct ElectionListenerTask;

impl BackgroundTask for ElectionListenerTask {
    fn name(&self) -> &'static str {
        "election-listener"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let election_service = ctx.state.presence.elections.clone();
            if let Err(e) = election_service.run_listener().await {
                tracing::error!(error = ?e, "Election service listener failed");
            }

            TaskOutcome::Completed
        })
    }
}
