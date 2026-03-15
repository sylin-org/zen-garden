//! Domain trait boundaries.
//!
//! Traits defined here replace direct infra imports in domain code.
//! Infra implements these traits; domain depends only on the trait.

pub mod ceremony_persistence;
pub mod docker_config;
pub mod harvest;
pub mod pond_client;
pub mod service_detector;
pub mod service_runtime;

pub use ceremony_persistence::CeremonyPersistence;
pub use docker_config::DockerConfigOps;
pub use harvest::HarvestOps;
pub use pond_client::PondClient;
pub use service_detector::ServiceDetector;
pub use service_runtime::ServiceRuntime;
