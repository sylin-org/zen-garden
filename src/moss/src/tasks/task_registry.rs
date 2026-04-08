//! Task registry — single source of truth for all background tasks (ARCH-0015).
//!
//! `build_task_registry()` returns every background task the supervisor should
//! run. Adding a task = adding one line. Removing a task = removing one line.

use super::task_defs::*;
use super::task_trait::BackgroundTask;

use crate::infra::AdoptionConfig;

/// Configuration for conditional task inclusion.
pub struct TaskConfig {
    pub adoption_config: AdoptionConfig,
    pub use_static_host: bool,
    pub mdns_available: bool,
}

/// Pre-created channels and objects that specific tasks need at construction time.
pub struct TaskChannels {
    pub vol_rx: tokio::sync::mpsc::Receiver<crate::infra::storage::monitor::PhysicalStorageEvent>,
    pub rescan_rx: tokio::sync::mpsc::Receiver<()>,
    pub bank: std::sync::Arc<crate::domain::StorageBank>,
    pub volumes: crate::domain::Volumes,
    pub pulse: tokio::sync::broadcast::Sender<crate::infra::PulseEvent>,
    pub notifications: std::sync::Arc<garden_common::notifications::NotificationRegistry>,
    pub monitor_token: tokio_util::sync::CancellationToken,
    pub watcher_set: crate::infra::storage::StorageWatcherSet,
    pub mdns_lurk_rx: Option<tokio::sync::broadcast::Receiver<garden_common::infra::koi_client::DiscoveredStone>>,
    pub self_stone_name: String,
}

/// Build the complete task registry.
///
/// Every background task is listed here. No second path, no duplication.
/// Conditional tasks are gated by `config` flags; tasks with owned state
/// consume the corresponding field from `channels`.
pub fn build_task_registry(config: TaskConfig, channels: TaskChannels) -> Vec<Box<dyn BackgroundTask>> {
    let mut tasks: Vec<Box<dyn BackgroundTask>> = vec![
        // ── Always-on unit-struct tasks ─────────────────────────────────
        Box::new(ElectionListenerTask),
        Box::new(DiscoveryHandlerTask),
        Box::new(HardwareDetectionTask),
        Box::new(TopologyProbeTask),
        Box::new(RegistryLoaderTask),
        Box::new(CatalogBuilderTask),
        Box::new(MetricsCollectorTask),
        Box::new(CompanionScanTask),
        Box::new(PresenceLoadMonitorTask),
        Box::new(PresenceHealthMonitorTask),
        Box::new(PondEnrollmentListenerTask),
        Box::new(InitialServiceSyncTask),
        Box::new(PeriodicAnnouncerTask),
        Box::new(HealthMonitorTask),
        Box::new(DockerEventsTask),
        Box::new(MediaWatcherTask),
        Box::new(TopologyMaintenanceTask),
        Box::new(RegistryMaintenanceTask),
        Box::new(StorageLifecycleTask),
        Box::new(S3ListenerLifecycleTask),
        Box::new(StorageConsoleTask),
        Box::new(OfferingOrchestrationTask),
        Box::new(StorageOrchestrationTask),
        Box::new(StorageTickAggregatorTask),
        Box::new(StorageReplicationTask),
        Box::new(MaintenanceSweepTask),
        Box::new(TaskSchedulerTask),
    ];

    // ── Pattern C: tasks with owned state ───────────────────────────────

    tasks.push(Box::new(VolumeMonitorTask {
        vol_rx: channels.vol_rx,
        rescan_rx: channels.rescan_rx,
        bank: channels.bank,
        volumes: channels.volumes,
        pulse: channels.pulse,
        notifications: channels.notifications,
        monitor_token: channels.monitor_token,
    }));

    tasks.push(Box::new(FsWatcherTask {
        watcher_set: channels.watcher_set,
    }));

    // ── Conditional tasks ───────────────────────────────────────────────

    if !config.use_static_host {
        tasks.push(Box::new(IpChangeHandlerTask));
    }

    if config.mdns_available {
        tasks.push(Box::new(MdnsHealthListenerTask));
    }

    if let Some(rx) = channels.mdns_lurk_rx {
        tasks.push(Box::new(MdnsLurkListenerTask {
            rx,
            self_stone_name: channels.self_stone_name,
        }));
    }

    if config.adoption_config.is_enabled() {
        tasks.push(Box::new(AutoAdoptionTask {
            config: config.adoption_config,
        }));
    }

    tasks
}
