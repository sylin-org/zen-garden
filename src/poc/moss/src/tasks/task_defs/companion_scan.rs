//! BackgroundTask: companion-scan (ARCH-0015)
//!
//! One-shot task that discovers and auto-starts companions.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct CompanionScanTask;

impl BackgroundTask for CompanionScanTask {
    fn name(&self) -> &'static str {
        "companion-scan"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let endpoint = format!("http://127.0.0.1:{}", garden_common::constants::MOSS_HTTP);
            match ctx
                .state
                .companion
                .registry
                .scan_and_autostart(&endpoint)
                .await
            {
                Ok((registered, started)) => {
                    tracing::info!(
                        registered = registered,
                        started = started,
                        "Companion scan and auto-start complete"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "Companion scan failed");
                }
            }

            TaskOutcome::Completed
        })
    }
}
