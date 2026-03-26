//! Service lifecycle management (legacy module)
//!
//! The `ServiceLifecycle` and `InstallServiceExecutor` types that lived here
//! were never wired into production. Real service installation is handled by
//! `tasks::job_executors::install_service_task()`, and lifecycle operations
//! (start, stop, restart, cordon) are in `domain::service_lifecycle`.
//!
//! This module is kept as a re-export point for `ServiceLifecycle` if any
//! downstream code references it by path; the struct is intentionally empty.

// No public API — all service lifecycle logic lives in:
//   domain::service_lifecycle  (stop, start, cordon, uncordon, destroy, remove, nourish)
//   tasks::job_executors       (install_service_task, install_batch_task, install_image_direct_task)
