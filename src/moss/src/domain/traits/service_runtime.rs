//! Container runtime trait for service lifecycle.

use anyhow::Result;
use garden_common::{ContainerResources, ServiceHealthStatus, ServiceStatus};
use std::future::Future;

/// Service lifecycle operations on the container runtime.
///
/// Abstracts Docker/Podman specifics from the domain layer.
pub trait ServiceRuntime: Send + Sync {
    /// Check if the container runtime is healthy.
    fn is_healthy(&self) -> impl Future<Output = bool> + Send;

    /// Check if a service container exists.
    fn service_exists(&self, service_name: &str) -> impl Future<Output = Result<bool>> + Send;

    /// Get service status (Running, Stopped, etc.).
    fn get_service_status(
        &self,
        service_name: &str,
    ) -> impl Future<Output = Result<ServiceStatus>> + Send;

    /// Get service health status (Healthy, Degraded, Offline).
    fn get_service_health(
        &self,
        service_name: &str,
    ) -> impl Future<Output = Result<ServiceHealthStatus>> + Send;

    /// List all zen service containers.
    fn list_services(&self) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Get container resource usage.
    fn get_stats(
        &self,
        service_name: &str,
    ) -> impl Future<Output = Result<ContainerResources>> + Send;

    /// Start a service container.
    fn start_service(&self, service_name: &str) -> impl Future<Output = Result<()>> + Send;

    /// Stop a service container.
    fn stop_service(&self, service_name: &str) -> impl Future<Output = Result<()>> + Send;

    /// Restart a service container.
    fn restart_service(&self, service_name: &str) -> impl Future<Output = Result<()>> + Send;

    /// Remove a service container.
    fn remove_service(&self, service_name: &str) -> impl Future<Output = Result<()>> + Send;
}
