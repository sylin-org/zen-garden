//! BackgroundTask: task-scheduler (ARCH-0015)
//!
//! Runs the scheduled task scheduler loop (cron-style background jobs).
//! Delegates to `task_scheduler::start_task_scheduler` which spawns internally.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct TaskSchedulerTask;

impl BackgroundTask for TaskSchedulerTask {
    fn name(&self) -> &'static str {
        "task-scheduler"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let handle = crate::tasks::task_scheduler::start_task_scheduler(
                ctx.state,
                ctx.token,
            );

            // start_task_scheduler returns a JoinHandle — await it so we stay alive
            // until shutdown or the scheduler exits.
            let _ = handle.await;

            TaskOutcome::Cancelled
        })
    }
}
