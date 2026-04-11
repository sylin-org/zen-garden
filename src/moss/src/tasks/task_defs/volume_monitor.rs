//! BackgroundTask: volume-monitor (ARCH-0015, STORAGE-0014)
//!
//! Cross-platform volume monitor. Receives physical storage events from the
//! platform monitor and processes them through StorageBank. Also handles
//! ad-hoc rescan requests (e.g. after `storage add`).
//!
//! Pattern C: carries the volume event receiver, rescan receiver, and
//! domain objects captured at construction time.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::domain::StorageBank;
use crate::infra;
use crate::infra::storage::monitor::PhysicalStorageEvent;
use crate::infra::storage::{ContentStore, OsPlatform};
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

use garden_common::notifications::{NOTIF_SOURCE_CANDIDATES, NotificationTag};

/// Volumes type alias matching `domain::storage::collection::Volumes`.
type Volumes =
    Arc<tokio::sync::RwLock<std::collections::HashMap<String, crate::domain::storage::Volume>>>;

pub struct VolumeMonitorTask {
    pub vol_rx: tokio::sync::mpsc::Receiver<PhysicalStorageEvent>,
    pub rescan_rx: tokio::sync::mpsc::Receiver<()>,
    pub bank: Arc<StorageBank>,
    pub volumes: Volumes,
    pub pulse: tokio::sync::broadcast::Sender<infra::PulseEvent>,
    pub notifications: Arc<garden_common::notifications::NotificationRegistry>,
    pub monitor_token: tokio_util::sync::CancellationToken,
}

impl BackgroundTask for VolumeMonitorTask {
    fn name(&self) -> &'static str {
        "volume-monitor"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        let mut vol_rx = self.vol_rx;
        let mut rescan_rx = self.rescan_rx;
        let bank = self.bank;
        let volumes = self.volumes;
        let pulse = self.pulse;
        let notifications = self.notifications;
        let monitor_token = self.monitor_token;

        Box::pin(async move {
            ctx.ready.signal();

            loop {
                tokio::select! {
                    _ = monitor_token.cancelled() => break,
                    event = vol_rx.recv() => {
                        let Some(ev) = event else { break };
                        match ev {
                            PhysicalStorageEvent::Connected { device_path, mount_path, label, capacity_bytes, used_bytes, removable } => {
                                let capacity_gb = capacity_bytes / 1_000_000_000;
                                let _ = pulse.send(infra::PulseEvent::Domain(
                                    infra::DomainPulse::storage_event(
                                        "storage_detected",
                                        format!("Volume appeared: {} ({})", mount_path.display(), label.as_deref().unwrap_or("unlabeled")),
                                        "info",
                                        None,
                                        Some(serde_json::json!({
                                            "device_path": device_path,
                                            "mount_path": mount_path,
                                            "label": label,
                                            "capacity_gb": capacity_gb,
                                            "removable": removable,
                                        })),
                                    )
                                ));
                                bank.on_appeared(device_path, mount_path, label, capacity_bytes, used_bytes, removable).await;
                            }
                            PhysicalStorageEvent::Disconnected { path } => {
                                let _ = pulse.send(infra::PulseEvent::Domain(
                                    infra::DomainPulse::storage_event(
                                        "storage_removed",
                                        format!("Volume disappeared: {}", path),
                                        "info",
                                        None,
                                        Some(serde_json::json!({ "path": path })),
                                    )
                                ));
                                bank.on_vanished(path).await;
                            }
                        }

                        // Update candidates notification
                        let candidate_count = {
                            let map = volumes.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
                                .count()
                        };
                        notifications.set_if(
                            NOTIF_SOURCE_CANDIDATES,
                            NotificationTag::Opportunity,
                            candidate_count > 0,
                        );
                    }
                    _ = rescan_rx.recv() => {
                        // Ad-hoc rescan requested (e.g. after `storage add` wrote a manifest).
                        let snaps = tokio::task::spawn_blocking(
                            crate::infra::storage::platform::scan_volumes
                        )
                        .await
                        .unwrap_or_default();
                        let make_store = |path: PathBuf| -> Arc<ContentStore> {
                            Arc::new(ContentStore::new(path, None))
                        };
                        crate::domain::storage::reconcile(&volumes, &snaps, &make_store).await;
                        crate::domain::storage::observe_all(&volumes, &OsPlatform).await;

                        let candidate_count = {
                            let map = volumes.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
                                .count()
                        };
                        notifications.set_if(
                            NOTIF_SOURCE_CANDIDATES,
                            NotificationTag::Opportunity,
                            candidate_count > 0,
                        );
                        tracing::debug!("Ad-hoc volume rescan complete");
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
