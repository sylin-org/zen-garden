//! BackgroundTask: offering-orchestration (ARCH-0015, ORCH-0001)
//!
//! Long-running task that manages Primary/Dormant/Joining/Degraded lifecycle
//! for replicated offerings.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct OfferingOrchestrationTask;

impl BackgroundTask for OfferingOrchestrationTask {
    fn name(&self) -> &'static str {
        "offering-orchestration"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["catalog-builder"]
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

            if let Err(e) = crate::tasks::offering_orchestration::offering_orchestration_task(
                ctx.state, ctx.token,
            )
            .await
            {
                tracing::error!(error = ?e, "Offering orchestration task failed");
                return TaskOutcome::Failed {
                    error: format!("{e:#}"),
                };
            }
            TaskOutcome::Cancelled
        })
    }
}
