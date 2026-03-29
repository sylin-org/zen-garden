//! Background tasks — discovery, health checks, gateway, metrics.
//!
//! All tasks follow the `tokio::select!` pattern with shutdown cancellation.

pub mod cloud_sync;
pub mod discovery;
pub mod gateway_announce;
pub mod health_check;
pub mod metrics_flush;
pub mod metrics_processor;
