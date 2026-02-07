//! Background task layer
//!
//! Long-running async tasks that run in the background:
//! - Service installation jobs (single and batch)
//! - Health monitoring loop
//! - Hardware capability detection
//! - Service discovery (Lantern registration)
//! - Network monitoring (IP change detection)
//! - Docker monitoring (daemon availability)
//! - Topology announcements (periodic stone presence)
//! - Task coordination (orchestrates all background tasks)
//! - Nurturing scheduler (automated A/B backups with seed bank replication)
//!
//! All tasks are non-blocking and composable.
//! Spawn with tokio::spawn() and communicate via channels/shared state.

pub mod announcer;
pub mod auto_adoption;
pub mod coordinator;
pub mod discovery;
pub mod discovery_handler;
pub mod docker_monitor;
pub mod election_service;
pub mod hardware_detection;
pub mod health_monitor;
pub mod job_executors;
pub mod metrics_collector;
pub mod network_monitor;
pub mod nurturing_scheduler;
pub mod presence_monitor;
pub mod state_provider;
pub mod task_scheduler;

pub use announcer::start_periodic_announcer;
pub use auto_adoption::auto_adoption_task;
pub use coordinator::{
    start_all_background_tasks,
    start_discovery_listener, start_hardware_detection,
    start_registry_loader, start_catalog_builder,
    start_health_monitor, start_auto_adoption, start_auto_adoption_with_config,
    start_lantern_registration, start_topology_maintenance,
    start_storage_maintenance,
};
pub use discovery::lantern_registration_loop;
pub use hardware_detection::detect_capabilities_background;
pub use health_monitor::health_monitor_task;
pub use job_executors::{
    install_service_task, install_batch_task,
    build_guidance, backfill_missing_guidance,
    build_adopted_guidance,
    refresh_capabilities_task, add_capability_task,
};
pub use metrics_collector::run_metrics_collector;
pub use network_monitor::{NetworkMonitor, NetworkMonitorConfig, NetworkEvent};
pub use docker_monitor::{DockerMonitor, DockerMonitorConfig, DockerEvent};
pub use nurturing_scheduler::{
    NurturingScheduler, NurturingWorkflowConfig, NurturingWorkflowResult,
    ReplicationAttempt, RoutingStrategy, trigger_nurturing, trigger_all_nurturing,
};
pub use task_scheduler::{
    TaskSchedulerConfig, start_task_scheduler, backfill_missing_tasks,
    run_scheduler_iteration,
};
