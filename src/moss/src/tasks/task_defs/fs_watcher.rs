//! BackgroundTask: fs-watcher (ARCH-0015, STORAGE-0009 Phase 5, STORAGE-0013)
//!
//! Detects external writes to managed storage mounts and records changelog
//! entries so replication stays coherent. Event-driven with a 60s heartbeat.
//!
//! Pattern C: carries the `StorageWatcherSet` constructed at registration time.

use std::future::Future;
use std::pin::Pin;

use crate::infra::storage::StorageWatcherSet;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct FsWatcherTask {
    pub watcher_set: StorageWatcherSet,
}

impl BackgroundTask for FsWatcherTask {
    fn name(&self) -> &'static str {
        "fs-watcher"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["volume-monitor"]
    }

    fn run(self: Box<Self>, mut ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        let watcher_set = self.watcher_set;

        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }
            ctx.ready.signal();

            let mut storage_rx = ctx.state.subscribe_storage_changed();
            let heartbeat = tokio::time::Duration::from_secs(60);

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => break,
                    result = storage_rx.recv() => {
                        match result {
                            Ok(event) => {
                                tracing::debug!(event = ?event, "fs watcher: storage event");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::debug!(skipped = n, "fs watcher: lagged, reconciling");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                        watcher_set.reconcile().await;
                    }
                    _ = tokio::time::sleep(heartbeat) => {
                        watcher_set.reconcile().await;
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
