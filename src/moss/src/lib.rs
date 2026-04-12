//! Zen Moss Library
//! Service discovery daemon for self-hosted infrastructure
//!
//! Architecture:
//! - domain/  - Business logic (service management, registry)
//! - infra/   - I/O operations (container runtime, filesystem, auth)
//! - api/     - HTTP handlers and routes
//! - tasks/   - Background async tasks (health monitor, job executors)
//! - bootstrap/ - Startup and initialization logic

pub mod api;
pub mod bootstrap;
pub mod cli;
pub mod domain;
pub mod infra;
pub mod tasks;

// Core modules
pub mod announcement;
pub mod docker;
pub mod http;
// mdns relocated to domain/discovery/mdns.rs in Book X (ARCH-0028)
pub use domain::discovery::mdns;

// App state for HTTP handlers
pub mod app_state;

// Test support — available to integration tests (tests/ directory)
pub mod testing;

// Re-export AppState
pub use app_state::AppState;

// Re-export domain types used by external consumers (rake, tests)
pub use domain::jobs::{Job, JobStatus};
pub use domain::{CompiledOffering, OfferingsFingerprint, OfferingsIndex};

// Re-export offering types from garden_common
pub use garden_common::{
    AdoptedData, BorrowedData, ManagedData, Offering, OfferingLocation, OfferingMode,
    OfferingModeData, OfferingStatus,
};

// Re-export API helpers from infra
pub use infra::api_helpers::{
    bad_gateway, bad_request, conflict, forbidden, internal, not_found, not_implemented,
    unavailable,
};
pub use infra::error_response;

// Re-export commonly used job event utilities
pub use api::v1::events::{
    emit_job_completed, emit_job_failed, emit_job_progress, emit_job_started,
};

// Re-export common types for convenience
pub use garden_common::ServiceInfo;

// Re-export domain functions and types
pub use domain::{adopt_existing_containers, adopt_offering_container, reconcile_services};

// Re-export tasks for background execution
pub use tasks::{
    DockerEvent, DockerMonitor, DockerMonitorConfig, Network, NetworkConfig, NetworkEvent,
    auto_adoption_task, detect_capabilities_background, health_monitor_task, install_batch_task,
    install_image_direct_task, install_service_task, lantern_registration_loop,
};

// Re-export bootstrap utilities
#[cfg(target_os = "windows")]
pub use bootstrap::ensure_windows_stone_name_config;
pub use bootstrap::{
    // Configuration
    DaemonConfig,
    // Startup utilities
    DockerConfig,
    PreInstallManifest,
    ServerConfig,
    bind_server,
    connect_docker,
    init_capabilities,
    init_tracing,
    load_preinstall_manifest,
    router,
    // Main orchestration
    run_daemon,
    run_first_boot_initialization,
    run_server,
};

// Re-export CLI utilities
pub use cli::{Cli, Commands, parse as parse_cli, version_string};
