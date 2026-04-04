//! BackgroundTask: hardware-detection (ARCH-0015)
//!
//! Delegates to the existing `detect_capabilities_background` function.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct HardwareDetectionTask;

impl BackgroundTask for HardwareDetectionTask {
    fn name(&self) -> &'static str {
        "hardware-detection"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            let stone_name = ctx.state.current.stone.name.clone();
            let capabilities = ctx.state.current.capabilities.clone();
            let console = ctx.state.console.clone();

            crate::tasks::hardware_detection::detect_capabilities_background(
                stone_name,
                capabilities,
                console,
                ctx.state,
            )
            .await;

            ctx.ready.signal();
            TaskOutcome::Completed
        })
    }
}
