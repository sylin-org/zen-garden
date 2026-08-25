//! BackgroundTask: media-watcher (ARCH-0015)
//!
//! Periodically scans physical media (disks) and reconciles the media map.
//! Uses `spawn_blocking` for the platform scan (PowerShell/lsblk).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct MediaWatcherTask;

impl BackgroundTask for MediaWatcherTask {
    fn name(&self) -> &'static str {
        "media-watcher"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            interval.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Media watcher shutting down");
                        break;
                    }
                }

                let snapshots =
                    tokio::task::spawn_blocking(crate::infra::storage::platform::scan_media)
                        .await
                        .unwrap_or_default();

                crate::domain::storage::reconcile_media(
                    &ctx.state.current.storage.media,
                    &snapshots,
                )
                .await;
            }

            TaskOutcome::Cancelled
        })
    }
}
