//! Health probe port and adapters.
//!
//! The `HealthProbe` trait abstracts the mechanism for checking whether
//! a container-backed offering is alive and healthy. The production
//! adapter wraps Docker container inspection; tests use a fake with
//! configurable responses.

use anyhow::Result;
use garden_common::{OfferingStatus, ServiceHealthStatus};
use std::future::Future;
use std::pin::Pin;

/// Result of a health probe for a single offering.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthProbeResult {
    pub status: OfferingStatus,
    pub health: ServiceHealthStatus,
}

/// Port: executes a health probe for a named offering.
///
/// Production adapter: [`DockerHealthProbe`]. Test adapter: see tests module.
pub trait HealthProbe: Send + Sync {
    /// Probe the offering by name. Returns the current status and health.
    fn probe<'a>(
        &'a self,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<HealthProbeResult>> + Send + 'a>>;
}

/// Production adapter: probes offering health via Docker container inspection.
pub struct DockerHealthProbe {
    docker: std::sync::Arc<crate::docker::Client>,
}

impl DockerHealthProbe {
    pub fn new(docker: std::sync::Arc<crate::docker::Client>) -> Self {
        Self { docker }
    }
}

impl HealthProbe for DockerHealthProbe {
    fn probe<'a>(
        &'a self,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<HealthProbeResult>> + Send + 'a>> {
        Box::pin(async move {
            let service_status = self.docker.get_service_status(name).await?;
            let health = self
                .docker
                .get_service_health(name)
                .await
                .unwrap_or(ServiceHealthStatus::Offline);

            Ok(HealthProbeResult {
                status: OfferingStatus::from(service_status),
                health,
            })
        })
    }
}
