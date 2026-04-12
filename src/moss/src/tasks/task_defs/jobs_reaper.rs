//! BackgroundTask: jobs-reaper (ARCH-0021 Book IV Ch5)
//!
//! Periodically sweeps terminal jobs (`Completed` / `Failed`) past the
//! terminal TTL out of the `Jobs` aggregate. Active jobs are never
//! evicted — a stuck `Pending` / `Running` job is a bug worth
//! surfacing, not a memory leak to hide. The TTL is enforced by
//! [`crate::domain::jobs::maintenance`] and defaults to 24 hours.
//!
//! This task replaces the "jobs accumulate forever" memory-leak class
//! identified in Chapter 1 of ARCH-0021: production stones that
//! complete hundreds of jobs per day used to drift unbounded since
//! nothing ever removed finished jobs from the in-memory map.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

/// Period between reaper sweeps.
///
/// 10 minutes balances responsiveness (operators see evictions
/// within a bounded window) against wasted work (the aggregate
/// holds a short-lived write lock per sweep).
const REAPER_INTERVAL_SECS: u64 = 10 * 60;

pub struct JobsReaperTask;

impl BackgroundTask for JobsReaperTask {
    fn name(&self) -> &'static str {
        "jobs-reaper"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(REAPER_INTERVAL_SECS));
            interval.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Jobs reaper shutting down");
                        break;
                    }
                }

                let report = ctx.state.jobs.maintain().await;
                if !report.is_empty() {
                    tracing::info!(
                        evicted = report.evicted,
                        kept = report.kept,
                        "Jobs reaper swept terminal jobs"
                    );
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
