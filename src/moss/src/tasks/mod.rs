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

pub mod adoption;
pub mod announcer;
pub mod auto_adoption;
pub mod coordinator;
pub mod discovery;
pub mod docker_events;
pub mod discovery_handler;
pub mod docker;
pub mod election_service;
pub mod hardware_detection;
pub mod health_monitor;
pub mod job_executors;
pub mod lantern;
pub mod metrics_collector;
pub mod network_monitor;
pub mod nurturing_scheduler;
pub mod offering_orchestration;
pub mod offering_reconciliation;
pub mod presence_monitor;
pub mod registry;
pub mod state_provider;
pub mod storage_orchestration;
pub mod storage_replication;
pub mod storage_tasks;
pub mod storage_tick_aggregator;
pub mod supervisor;
pub mod task_defs;
pub mod task_registry;
pub mod task_scheduler;
pub mod task_trait;
pub mod topology;
pub mod topology_probe;

pub use announcer::start_periodic_announcer;
pub use auto_adoption::auto_adoption_task;
pub use lantern::start_lantern_registration;
pub use discovery::lantern_registration_loop;
pub use docker::{Config as DockerMonitorConfig, Event as DockerEvent, Monitor as DockerMonitor};
pub use docker_events::docker_events_task;
pub use hardware_detection::detect_capabilities_background;
pub use health_monitor::health_monitor_task;
pub use job_executors::{
    add_capability_task, backfill_missing_guidance, build_adopted_guidance, build_guidance,
    install_batch_task, install_image_direct_task, install_service_task, refresh_capabilities_task,
};
pub use metrics_collector::run_metrics_collector;
pub use network_monitor::{Network, NetworkConfig, NetworkEvent};
pub use nurturing_scheduler::{
    trigger_all_nurturing, trigger_nurturing, NurturingScheduler, NurturingWorkflowConfig,
    NurturingWorkflowResult, ReplicationAttempt, RoutingStrategy,
};
pub use task_scheduler::{
    backfill_missing_tasks, run_scheduler_iteration, start_task_scheduler, TaskSchedulerConfig,
};
