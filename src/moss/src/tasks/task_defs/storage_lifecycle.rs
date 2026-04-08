//! BackgroundTask: storage-lifecycle (ARCH-0015, STORAGE-0011, STORAGE-0018)
//!
//! Long-running 10s interval task that handles auto-mount and health ticks.
//!
//! Storage announcements are event-driven, not periodic:
//! - Connect/disconnect → `emit_storage_changed()` → tools projection + nudge
//! - New stone discovery → snapshot response (announcer / discovery handler)
//! - 60s tools snapshot beacon (periodic-announcer) as catch-up
//!
//! This task does NOT broadcast storage beacons or refresh tools projection.
//! Those happen reactively via `emit_storage_changed()`.

use std::future::Future;
use std::pin::Pin;

use crate::infra::storage::OsPlatform;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct StorageLifecycleTask;

impl BackgroundTask for StorageLifecycleTask {
    fn name(&self) -> &'static str {
        "storage-lifecycle"
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

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.tick().await;
            tracing::info!("Storage lifecycle task started (10s interval)");

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Storage lifecycle shutting down");
                        break;
                    }
                }

                // Auto-mount unmounted managed devices. No event emitted —
                // VolumeMonitor detects the mount and calls on_appeared(),
                // which emits Connected with the correct state.
                crate::domain::storage::auto_mount_unmounted(&OsPlatform).await;

                // Health tick: probe disk usage + device health (STORAGE-0018).
                // State transitions (degrade, disconnect) emit StorageChanged
                // events which flow through emit_storage_changed() → tools
                // projection + orchestration nudge.
                crate::domain::storage::observe_all(
                    &ctx.state.current.storage.volumes,
                    &OsPlatform,
                )
                .await;
            }

            TaskOutcome::Cancelled
        })
    }
}
