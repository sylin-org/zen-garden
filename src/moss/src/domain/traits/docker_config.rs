//! Docker daemon configuration trait.

use anyhow::Result;
use async_trait::async_trait;

/// Operations on the Docker daemon's configuration (daemon.json).
///
/// Used by the infrastructure handler to manage insecure-registries
/// when container registries are deployed in the garden.
#[async_trait]
pub trait DockerConfigOps: Send + Sync {
    /// Read the current insecure-registries list from daemon.json.
    async fn read_insecure_registries(&self) -> Result<Vec<String>>;

    /// Write insecure-registries to daemon.json. Returns true if changed.
    async fn write_insecure_registries(&self, registries: &[String]) -> Result<bool>;

    /// Read the garden-managed registries state file.
    async fn read_garden_registries(&self) -> Vec<String>;

    /// Write the garden-managed registries state file.
    async fn write_garden_registries(&self, registries: &[String]) -> Result<()>;

    /// Restart the Docker daemon to apply configuration changes.
    async fn restart_docker_daemon(&self) -> Result<()>;
}
