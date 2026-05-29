//! BackgroundTask: capacity-reclaim (STORAGE-0020).
//!
//! Drives the [`Capacity`](crate::domain::Capacity) governor on a fixed
//! interval: measure the data filesystem, classify pressure, and — from
//! `Elevated` upward — run a reclaim pass. Surfaces a stone-level
//! `Attention` notification while pressure is `High` or `Critical`, so a
//! filling disk is never silent (the failure mode that let an earlier
//! snapshot runaway fill a stone unnoticed for days).
//!
//! This is the pressure-reactive counterpart to the age-based Caretaking
//! sweep: that one is scheduled hygiene, this one is a real-time invariant.

use std::future::Future;
use std::pin::Pin;

use garden_common::notifications::{NOTIF_SOURCE_SYSTEM_CRITICAL, NotificationTag};

use crate::domain::Pressure;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

/// Period between governing cycles.
///
/// 60s keeps the `df` subprocess cost negligible while reacting to a
/// filling disk within a bounded window. Admission control (`reserve`)
/// measures fresh on the write path, so this cadence only bounds reclaim
/// latency, not the safety floor.
const RECLAIM_INTERVAL_SECS: u64 = 60;

pub struct CapacityReclaimTask;

impl BackgroundTask for CapacityReclaimTask {
    fn name(&self) -> &'static str {
        "capacity-reclaim"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(RECLAIM_INTERVAL_SECS));
            interval.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Capacity reclaim shutting down");
                        break;
                    }
                }

                let report = ctx.state.capacity.govern().await;

                // Surface (or clear) a stone-level attention notification.
                ctx.state.presence.notifications.set_if(
                    NOTIF_SOURCE_SYSTEM_CRITICAL,
                    NotificationTag::Attention,
                    report.pressure >= Pressure::High,
                );

                if let Some(run) = &report.reclaim {
                    let total = run.total_items();
                    if total > 0 {
                        tracing::info!(
                            pressure = report.pressure.name(),
                            level = ?run.level,
                            reclaimed_items = total,
                            "Capacity governor reclaimed disk space"
                        );
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
