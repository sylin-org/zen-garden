//! BackgroundTask: discovery-handler (ARCH-0015)
//!
//! Responds to p2p discovery requests from other stones.
//! Delegates to the existing `discovery_handler::start_discovery_handler`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct DiscoveryHandlerTask;

impl BackgroundTask for DiscoveryHandlerTask {
    fn name(&self) -> &'static str {
        "discovery-handler"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let state = Arc::new(ctx.state);
            if let Err(e) = crate::tasks::discovery_handler::start_discovery_handler(state).await {
                tracing::error!(error = ?e, "Discovery handler failed");
            }

            TaskOutcome::Completed
        })
    }
}
