//! Platform domain — Docker, runtime, network monitor, infrastructure handlers.

use std::sync::Arc;
use crate::docker::Client;
use crate::tasks::Network;
use crate::domain::InfrastructureHandlerRegistry;
use garden_common::PlatformRuntime;

/// Platform domain context (`state.platform`).
///
/// Groups the platform services that the stone daemon runs on top of:
/// the container runtime, the OS-level platform abstraction,
/// the network monitor, and the infrastructure change handlers.
#[derive(Clone)]
pub struct Platform {
    /// Docker/container runtime client.
    pub docker: Arc<Client>,

    /// Platform runtime — console/ribbon output and lifecycle signals (ARCH-0002).
    pub runtime: Arc<dyn PlatformRuntime>,

    /// Network monitor for IP change detection and network subsystem state.
    pub network: Arc<Network>,

    /// Infrastructure handlers — react to topology changes (registry trust, DNS, etc.)
    pub handlers: Arc<InfrastructureHandlerRegistry>,
}
