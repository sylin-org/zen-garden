//! Domain trait boundaries.
//!
//! Traits defined here replace direct infra imports in domain code.
//! Infra implements these traits; domain depends only on the trait.

pub mod ceremony_persistence;
pub mod companion_ops;
pub mod content_store;
pub mod docker_config;
pub mod harvest;
pub mod management_store;
pub mod nurturing_store;
pub mod pond_client;
pub mod service_detector;
pub mod storage_platform;
pub mod task_registry_persistence;

pub use ceremony_persistence::CeremonyPersistence;
pub use companion_ops::CompanionOps;
pub use content_store::ContentStoreOps;
pub use docker_config::DockerConfigOps;
pub use harvest::HarvestOps;
pub use management_store::ManagementStoreOps;
pub use nurturing_store::NurturingStoreOps;
pub use pond_client::PondClient;
pub use service_detector::ServiceDetector;
pub use storage_platform::StoragePlatform;
pub use task_registry_persistence::TaskRegistryPersistence;
