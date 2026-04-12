//! Platform domain — Docker, runtime, network monitor, infrastructure handlers.

use crate::docker::ContainerRuntime;
use crate::domain::InfrastructureHandlerRegistry;
use crate::tasks::Network;
use garden_common::PlatformRuntime;
use std::sync::Arc;

/// Platform domain context (`state.platform`).
///
/// Groups the platform services that the stone daemon runs on top of:
/// the container runtime, the OS-level platform abstraction,
/// the network monitor, and the infrastructure change handlers.
#[derive(Clone)]
pub struct Platform {
    /// Container runtime — abstracts Docker/Podman operations (ARCH-0030).
    pub container: Arc<ContainerRuntime>,

    /// Platform runtime — console/ribbon output and lifecycle signals (ARCH-0002).
    pub runtime: Arc<dyn PlatformRuntime>,

    /// Network monitor for IP change detection and network subsystem state.
    pub network: Arc<Network>,

    /// Infrastructure handlers — react to topology changes (registry trust, DNS, etc.)
    pub handlers: Arc<InfrastructureHandlerRegistry>,
}
