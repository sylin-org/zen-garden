//! BackgroundTask: offerings projection (ARCH-0016)
//!
//! Subscribes to the Offerings aggregate's `changes()` broadcast and reacts
//! to every mutation by:
//!
//! 1. Refreshing the local tools projection so the garden registry and
//!    tools beacon see the new offering set.
//! 2. Calling `sync_self_services` so peers receive an immediate topology
//!    chirp if the mutation warrants it.
//!
//! This task enforces the invariant "tool registry must be coherent
//! with offerings after every mutation" by subscription, not by
//! convention. The reconcile + publish step is delegated to
//! [`crate::domain::tool::projection::reproject_and_publish`] so the
//! task stays a thin dispatcher.
//!
//! Pattern A: unit-struct task.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::broadcast::error::RecvError;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct OfferingsProjectionTask;

impl BackgroundTask for OfferingsProjectionTask {
    fn name(&self) -> &'static str {
        "offerings-projection"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            // Subscribe BEFORE the initial refresh so no events are missed
            // in the window between seeding the projection and entering the
            // receive loop. If the aggregate fires an `OfferingsChanged`
            // while `reproject_and_publish` is running, the event is
            // buffered by the broadcast channel and processed on the
            // first `feed.recv().await` below.
            let mut feed = ctx.state.offerings.changes();

            // Seed the projection from current state. The aggregate may
            // already contain loaded offerings from disk at this point.
            crate::domain::tool::projection::reproject_and_publish(&ctx.state).await;
            ctx.ready.signal();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => {
                        return TaskOutcome::Cancelled;
                    }
                    msg = feed.recv() => match msg {
                        Ok(event) => {
                            tracing::debug!(
                                kind = ?event.kind,
                                affected = ?event.affected,
                                "OfferingsChanged — refreshing projection",
                            );
                            crate::domain::tool::projection::reproject_and_publish(&ctx.state).await;
                            crate::domain::topology::composition::sync_services(
                                &ctx.state,
                                event.kind.should_chirp(),
                            )
                            .await;
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped,
                                "offerings projection feed lagged — full reconcile",
                            );
                            // ARCH-0018: record lag for observability.
                            ctx.state
                                .metrics
                                .record_subscriber_lag("offerings-projection", skipped)
                                .await;
                            crate::domain::tool::projection::reproject_and_publish(&ctx.state).await;
                            crate::domain::topology::composition::sync_services(&ctx.state, true)
                                .await;
                        }
                        Err(RecvError::Closed) => {
                            tracing::info!("offerings projection feed closed");
                            return TaskOutcome::Completed;
                        }
                    }
                }
            }
        })
    }
}
