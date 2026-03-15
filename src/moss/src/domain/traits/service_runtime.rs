//! Container runtime trait for service lifecycle.

use anyhow::Result;
use async_trait::async_trait;
use garden_common::{ContainerResources, ServiceHealthStatus, ServiceStatus};

/// Service lifecycle operations on the container runtime.
///
/// Abstracts Docker/Podman specifics from the domain layer.
#[async_trait]
pub trait ServiceRuntime: Send + Sync {
    /// Check if the container runtime is healthy.
    async fn is_healthy(&self) -> bool;

    /// Check if a service container exists.
    async fn service_exists(&self, service_name: &str) -> Result<bool>;

    /// Get service status (Running, Stopped, etc.).
    async fn get_service_status(&self, service_name: &str) -> Result<ServiceStatus>;

    /// Get service health status (Healthy, Degraded, Offline).
    async fn get_service_health(&self, service_name: &str) -> Result<ServiceHealthStatus>;

    /// List all zen service containers.
    async fn list_services(&self) -> Result<Vec<String>>;

    /// Get container resource usage.
    async fn get_stats(&self, service_name: &str) -> Result<ContainerResources>;

    /// Start a service container.
    async fn start_service(&self, service_name: &str) -> Result<()>;

    /// Stop a service container.
    async fn stop_service(&self, service_name: &str) -> Result<()>;

    /// Restart a service container.
    async fn restart_service(&self, service_name: &str) -> Result<()>;

    /// Remove a service container.
    async fn remove_service(&self, service_name: &str) -> Result<()>;
}
