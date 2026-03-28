//! Background tasks.
//!
//! Each task is a long-lived async loop dispatching through the
//! [`Offering`](crate::catalog::Offering) trait or operating on domain
//! algorithms. No direct HTTP calls — always through offerings or infra.

pub mod benchmark;
pub mod cloud_sync;
pub mod discovery;
pub mod gateway_announce;
pub mod health_check;
pub mod metrics_flush;
pub mod metrics_processor;
pub mod placement;
pub mod reconciliation;
pub mod resource_sync;
pub mod snapshot_publisher;
