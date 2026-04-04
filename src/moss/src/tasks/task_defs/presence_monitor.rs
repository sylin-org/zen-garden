//! BackgroundTask: presence monitors (ARCH-0015)
//!
//! Two long-running tasks for the PRESENCE-0001 protocol:
//! - PresenceLoadMonitorTask: monitors system load for presence announcements
//! - PresenceHealthMonitorTask: monitors health transitions

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

// ── Load monitor ────────────────────────────────────────────────────────

pub struct PresenceLoadMonitorTask;

impl BackgroundTask for PresenceLoadMonitorTask {
    fn name(&self) -> &'static str {
        "presence-load-monitor"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();
            crate::tasks::presence_monitor::run_load_monitor_task(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}

// ── Health monitor ──────────────────────────────────────────────────────

pub struct PresenceHealthMonitorTask;

impl BackgroundTask for PresenceHealthMonitorTask {
    fn name(&self) -> &'static str {
        "presence-health-monitor"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();
            crate::tasks::presence_monitor::run_health_monitor_task(ctx.state, ctx.token).await;
            TaskOutcome::Cancelled
        })
    }
}
