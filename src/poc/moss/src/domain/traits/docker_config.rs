//! Docker daemon configuration trait.

use anyhow::Result;
use std::future::Future;

/// Operations on the Docker daemon's configuration (daemon.json).
///
/// Used by the infrastructure handler to manage insecure-registries
/// when container registries are deployed in the garden.
pub trait DockerConfigOps: Send + Sync {
    /// Read the current insecure-registries list from daemon.json.
    fn read_insecure_registries(&self) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Write insecure-registries to daemon.json. Returns true if changed.
    fn write_insecure_registries(
        &self,
        registries: &[String],
    ) -> impl Future<Output = Result<bool>> + Send;

    /// Read the garden-managed registries state file.
    fn read_garden_registries(&self) -> impl Future<Output = Vec<String>> + Send;

    /// Write the garden-managed registries state file.
    fn write_garden_registries(
        &self,
        registries: &[String],
    ) -> impl Future<Output = Result<()>> + Send;

    /// Restart the Docker daemon to apply configuration changes.
    fn restart_docker_daemon(&self) -> impl Future<Output = Result<()>> + Send;
}
