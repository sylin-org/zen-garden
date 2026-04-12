//! Discovery aggregate root.
//!
//! Encapsulates the Koi embedded handle and the mDNS service registration
//! handle. Provides typed commands for mDNS operations and a `changes()`
//! broadcast for domain event subscribers.

use std::sync::Arc;

use garden_common::infra::koi_client::DiscoveredStone;
use koi_embedded::KoiHandle;
use tokio::sync::broadcast;

use super::event::{DiscoveryChangeKind, DiscoveryChanged};
use super::mdns::MdnsHandle;
use crate::domain::Metrics;

/// Discovery aggregate — mDNS registration, Koi handle, peer discovery.
///
/// Ephemeral aggregate (no persistence). State is rebuilt on every process
/// start. mDNS registrations are volatile.
pub struct Discovery {
    /// Koi embedded handle — mDNS, DNS, certmesh, vault capabilities.
    koi: Arc<KoiHandle>,

    /// mDNS registration handle (None if mDNS unavailable at startup).
    mdns: Option<Arc<MdnsHandle>>,

    /// Lurk-listener broadcast source (mDNS browse for neighbor stones).
    lurk_tx: Option<broadcast::Sender<DiscoveredStone>>,

    /// Domain event channel.
    changed: broadcast::Sender<DiscoveryChanged>,

    /// Metrics integration (ARCH-0018).
    metrics: Arc<Metrics>,
}

impl Clone for Discovery {
    fn clone(&self) -> Self {
        Self {
            koi: self.koi.clone(),
            mdns: self.mdns.clone(),
            lurk_tx: self.lurk_tx.clone(),
            changed: self.changed.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl Discovery {
    /// Construct a new Discovery aggregate.
    ///
    /// Called once during bootstrap. The `mdns` handle is `None` when
    /// mDNS is unavailable (e.g., no network at startup).
    pub async fn new(
        koi: Arc<KoiHandle>,
        mdns: Option<Arc<MdnsHandle>>,
        lurk_tx: Option<broadcast::Sender<DiscoveredStone>>,
        metrics: Arc<Metrics>,
    ) -> Self {
        metrics
            .register_domain("discovery", DiscoveryChangeKind::ALL_NAMES)
            .await;

        let (changed, _) = broadcast::channel(64);

        Self {
            koi,
            mdns,
            lurk_tx,
            changed,
            metrics,
        }
    }

    // ========================================================================
    // Commands (write)
    // ========================================================================

    /// Re-register the mDNS `_moss._tcp` and `_http._tcp` services.
    ///
    /// Called on IP/MAC changes and at initial registration.
    /// Infallible — mDNS errors are logged and swallowed (non-fatal).
    pub async fn reregister(&self, ip: &str, mac: Option<&str>) {
        let Some(ref mdns) = self.mdns else {
            tracing::debug!("mDNS reregister skipped — no mDNS handle");
            return;
        };

        if let Err(e) = mdns.reregister(ip, mac).await {
            tracing::warn!(error = ?e, ip = %ip, "mDNS re-registration failed");
            return;
        }

        self.emit(DiscoveryChangeKind::Registered).await;
    }

    /// Update the mDNS TXT record with a new health status.
    ///
    /// Called when stone health transitions (e.g., thriving → withering).
    pub async fn update_health(&self, health: &str) {
        let Some(ref mdns) = self.mdns else {
            tracing::debug!("mDNS health update skipped — no mDNS handle");
            return;
        };

        mdns.update_health(health).await;
        self.emit(DiscoveryChangeKind::HealthUpdated).await;
    }

    /// Register the `_certmesh._tcp` CA service on mDNS.
    ///
    /// Only registers when this stone is an unlocked cornerstone.
    pub async fn register_certmesh(&self, port: u16) {
        super::mdns::register_certmesh_service(&self.koi, port).await;
        self.emit(DiscoveryChangeKind::CertmeshRegistered).await;
    }

    // ========================================================================
    // Queries (read)
    // ========================================================================

    /// Access the Koi embedded handle.
    ///
    /// Used by Security (certmesh), Storage (vault), and other domains
    /// that need Koi sub-handles. The handle stays on Discovery because
    /// it is a multi-capability embedded service.
    pub fn koi(&self) -> &Arc<KoiHandle> {
        &self.koi
    }

    /// Whether mDNS is currently registered.
    pub fn mdns_registered(&self) -> bool {
        self.mdns.as_ref().is_some_and(|m| m.is_registered())
    }

    /// Whether an mDNS handle is available.
    pub fn has_mdns(&self) -> bool {
        self.mdns.is_some()
    }

    /// Subscribe to the lurk-listener stream (mDNS browse for peers).
    ///
    /// Returns `None` if mDNS lurk-listener was not started.
    pub fn lurk_stream(&self) -> Option<broadcast::Receiver<DiscoveredStone>> {
        self.lurk_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Subscribe to discovery domain events.
    pub fn changes(&self) -> broadcast::Receiver<DiscoveryChanged> {
        self.changed.subscribe()
    }

    // ========================================================================
    // Internal
    // ========================================================================

    async fn emit(&self, kind: DiscoveryChangeKind) {
        self.metrics
            .record_domain_event("discovery", kind.name())
            .await;

        let event = DiscoveryChanged {
            kind,
            timestamp: chrono::Utc::now(),
        };

        let _ = self.changed.send(event);
    }
}
