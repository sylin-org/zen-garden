//! BackgroundTask: storage-tick-aggregator (ARCH-0015, STORAGE-0006 Phase 4f)
//!
//! Long-running task that quantises raw per-write ticks into per-seed-bank
//! aggregated ticks (2s quiet threshold / 10s deadline cap).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageTickAggregatorTask;

impl BackgroundTask for StorageTickAggregatorTask {
    fn name(&self) -> &'static str {
        "storage-tick-aggregator"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["volume-monitor"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }
            ctx.ready.signal();

            let raw_rx = ctx.state.orchestration.storage.tick.raw.subscribe();
            let agg_tx = ctx.state.orchestration.storage.tick.debounced.clone();

            crate::tasks::storage_tick_aggregator::storage_tick_aggregator_task(
                raw_rx, agg_tx, ctx.token,
            )
            .await;

            TaskOutcome::Cancelled
        })
    }
}
