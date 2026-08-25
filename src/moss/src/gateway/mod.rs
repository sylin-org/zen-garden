//! `KoiGateway` — the seam between Moss and the koi trust fabric (GATEWAY-0001).
//!
//! Every koi interaction in moss flows through this port: pond status,
//! envelope sign/verify, the credential vault, CA cert location, and
//! trust-lifecycle events. Today it is implemented in-process over the
//! koi-embedded handle; after koi's standalone release, an HTTP-backed
//! implementation talks to the resident sidecar service — without any of
//! the port's consumers changing.
//!
//! Contract rules:
//! - Trait signatures use zen-owned DTOs (`PondStatus`, `GatewayEvent`) or
//!   koi-*protocol* types from `koi-common` (`Envelope`, `Assurance`) — the
//!   cross-repo wire contract. Handle types from `koi-embedded` /
//!   `koi-certmesh` never appear here, so a remote implementation can slot in.
//! - `sign_canonical` is the hot path (chirp + inter-stone envelopes). It is
//!   async so a sidecar implementation can batch or offload; embedded cost is
//!   a local key operation.
//! - Unavailable capability ≠ error crash: callers receive
//!   [`GatewayError::Unavailable`] and degrade exactly as they do today when
//!   certmesh is disabled.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use koi_common::envelope::{Assurance, Envelope};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

// ============================================================================
// DTOs — zen-owned, wire-ready (serde) for the future HTTP transport
// ============================================================================

/// Pond/trust status summary. Zen-owned mirror of the subset of certmesh
/// status that moss consumes (bootstrap seeding, pond API, renewal task).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PondStatus {
    pub initialized: bool,
    pub unlocked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(default)]
    pub enrollment_open: bool,
    #[serde(default)]
    pub member_count: usize,
}

/// Pond-relevant trust-lifecycle event, mapped from the koi event stream.
/// Variants intentionally mirror [`crate::domain::events::PondEvent`]
/// constructors; everything else (mDNS/DNS/proxy/runtime) is filtered at the
/// source, matching the current `tasks/koi_events.rs` semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayEvent {
    PostureChanged { signed: bool },
    CertRenewed { expires_at: DateTime<Utc> },
    CertExpiringSoon { days_left: i64 },
    RenewalFailed { reason: String, consecutive_failures: u32 },
}

/// Failure of a gateway operation. `Unavailable` means the backend has no such
/// capability right now (certmesh disabled, sidecar down); callers degrade.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("koi gateway capability unavailable: {0}")]
    Unavailable(String),
    #[error("koi gateway backend error: {0}")]
    Backend(String),
}

pub type GatewayResult<T> = Result<T, GatewayError>;

// ============================================================================
// Port
// ============================================================================

#[async_trait]
pub trait KoiGateway: Send + Sync {
    /// Current pond/trust status. `Unavailable` when certmesh is absent.
    async fn pond_status(&self) -> GatewayResult<PondStatus>;

    /// Path to the CA certificate PEM used as chirp-verification anchor.
    /// `None` when no certmesh core exists (open garden).
    fn ca_cert_path(&self) -> Option<PathBuf>;

    /// Sign canonical request bytes with this stone's identity. Infallible by
    /// koi contract (Open posture yields an unsigned passthrough envelope).
    async fn sign_canonical(&self, bytes: &[u8]) -> GatewayResult<Envelope>;

    /// Verify an envelope against this stone's pinned CA + revocation roster.
    async fn verify_envelope(&self, envelope: &Envelope) -> Assurance;

    /// Store a secret in the koi vault (machine-bound passphrase backend).
    fn vault_store(&self, key: &str, secret: &str) -> GatewayResult<()>;

    /// Subscribe to pond-relevant trust-lifecycle events.
    fn subscribe_events(&self) -> broadcast::Receiver<GatewayEvent>;
}

// ============================================================================
// Embedded adapter — wraps the koi-embedded handle (current era)
// ============================================================================

/// In-process implementation backed by the koi-embedded handle built during
/// bootstrap Phase 4. Cheap to clone/construct: it only holds the `Arc`.
pub struct EmbeddedKoiGateway {
    handle: Arc<koi_embedded::KoiHandle>,
    events_tx: OnceLock<broadcast::Sender<GatewayEvent>>,
}

impl EmbeddedKoiGateway {
    pub fn new(handle: Arc<koi_embedded::KoiHandle>) -> Self {
        Self {
            handle,
            events_tx: OnceLock::new(),
        }
    }

    fn core(
        &self,
    ) -> GatewayResult<Arc<koi_certmesh::CertmeshCore>> {
        self.handle
            .certmesh()
            .map_err(|e| GatewayError::Unavailable(format!("certmesh handle: {e}")))?
            .core()
            .map_err(|e| GatewayError::Unavailable(format!("certmesh core: {e}")))
    }

    /// Lazily start the koi-event → [`GatewayEvent`] forwarder on first
    /// subscription. One forwarder per gateway instance; lagged receivers
    /// surface as `RecvError::Lagged` at the consumer, never break the source.
    fn ensure_event_forwarder(&self) -> broadcast::Receiver<GatewayEvent> {
        let tx = self.events_tx.get_or_init(|| {
            let (tx, _) = broadcast::channel(64);
            let stream = self.handle.events();
            tokio::spawn(forward_events(stream, tx.clone()));
            tx
        });
        tx.subscribe()
    }
}

#[async_trait]
impl KoiGateway for EmbeddedKoiGateway {
    async fn pond_status(&self) -> GatewayResult<PondStatus> {
        let status = self.core()?.certmesh_status().await;
        Ok(PondStatus {
            initialized: status.ca_initialized,
            unlocked: !status.ca_locked,
            fingerprint: status.ca_fingerprint,
            auth_method: status.auth_method,
            enrollment_open: status.enrollment_open,
            member_count: status.member_count,
        })
    }

    fn ca_cert_path(&self) -> Option<PathBuf> {
        let core = self.core().ok()?;
        Some(core.paths().ca_cert_path())
    }

    async fn sign_canonical(&self, bytes: &[u8]) -> GatewayResult<Envelope> {
        Ok(self.core()?.sign(bytes).await)
    }

    async fn verify_envelope(&self, envelope: &Envelope) -> Assurance {
        // Verification is posture-transparent: Open gardens yield Anonymous
        // freshness verdicts, mirroring `CertmeshCore::verify`.
        match self.core() {
            Ok(core) => core.verify(envelope).await,
            Err(_) => Assurance::Anonymous {
                freshness: koi_common::envelope::Freshness::Stale,
            },
        }
    }

    fn vault_store(&self, key: &str, secret: &str) -> GatewayResult<()> {
        let vault = self
            .handle
            .vault()
            .map_err(|e| GatewayError::Unavailable(format!("vault handle: {e}")))?;
        vault
            .store(key, secret)
            .map_err(|e| GatewayError::Backend(format!("vault store: {e}")))
    }

    fn subscribe_events(&self) -> broadcast::Receiver<GatewayEvent> {
        self.ensure_event_forwarder()
    }
}

/// Bridge task: koi broadcast stream → pond-relevant gateway events.
async fn forward_events(
    mut stream: tokio_stream::wrappers::BroadcastStream<koi_embedded::KoiEvent>,
    tx: broadcast::Sender<GatewayEvent>,
) {
    use tokio_stream::StreamExt;
    while let Some(next) = stream.next().await {
        match next {
            Ok(event) => {
                if let Some(mapped) = map_koi_event(event) {
                    let _ = tx.send(mapped);
                }
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!(lagged = n, "koi gateway event stream lagged — continuing");
            }
        }
    }
}

/// Pure mapping from koi lifecycle events to pond domain events. Public within
/// the crate so `tasks/koi_events.rs` and tests share one definition.
pub(crate) fn map_koi_event(event: koi_embedded::KoiEvent) -> Option<GatewayEvent> {
    match event {
        koi_embedded::KoiEvent::PostureChanged { to, .. } => {
            Some(GatewayEvent::PostureChanged { signed: to.signed })
        }
        koi_embedded::KoiEvent::CertRenewed { expires_at } => {
            Some(GatewayEvent::CertRenewed { expires_at })
        }
        koi_embedded::KoiEvent::CertExpiringSoon { days_left } => {
            Some(GatewayEvent::CertExpiringSoon { days_left })
        }
        koi_embedded::KoiEvent::CertRenewalFailed {
            reason,
            consecutive_failures,
        } => Some(GatewayEvent::RenewalFailed {
            reason,
            consecutive_failures,
        }),
        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Test double proving the port is usable as a dyn trait object with no
    /// koi types at the call boundary — the shape a mock HTTP sidecar or a
    /// test harness would take.
    struct FakeKoiGateway {
        status: PondStatus,
        vault: Mutex<Vec<(String, String)>>,
        events_tx: broadcast::Sender<GatewayEvent>,
    }

    impl FakeKoiGateway {
        fn new(status: PondStatus) -> Self {
            let (events_tx, _) = broadcast::channel(8);
            Self {
                status,
                vault: Mutex::new(Vec::new()),
                events_tx,
            }
        }
    }

    #[async_trait]
    impl KoiGateway for FakeKoiGateway {
        async fn pond_status(&self) -> GatewayResult<PondStatus> {
            Ok(self.status.clone())
        }

        fn ca_cert_path(&self) -> Option<PathBuf> {
            None
        }

        async fn sign_canonical(&self, bytes: &[u8]) -> GatewayResult<Envelope> {
            Ok(Envelope {
                v: 1,
                payload: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
                nonce: "test-nonce".into(),
                ts: 0,
                sig: None,
            })
        }

        async fn verify_envelope(&self, _envelope: &Envelope) -> Assurance {
            Assurance::Anonymous {
                freshness: koi_common::envelope::Freshness::Fresh,
            }
        }

        fn vault_store(&self, key: &str, secret: &str) -> GatewayResult<()> {
            self.vault.lock().unwrap().push((key.into(), secret.into()));
            Ok(())
        }

        fn subscribe_events(&self) -> broadcast::Receiver<GatewayEvent> {
            self.events_tx.subscribe()
        }
    }

    fn active_pond_status() -> PondStatus {
        PondStatus {
            initialized: true,
            unlocked: true,
            fingerprint: Some("ab12".into()),
            auth_method: Some("totp".into()),
            enrollment_open: false,
            member_count: 3,
        }
    }

    #[tokio::test]
    async fn port_is_dyn_safe_and_fake_satisfies_it() {
        let gw: Arc<dyn KoiGateway> = Arc::new(FakeKoiGateway::new(active_pond_status()));

        let status = gw.pond_status().await.unwrap();
        assert_eq!(status, active_pond_status());

        let bytes = b"canonical-bytes";
        let env = gw.sign_canonical(bytes).await.unwrap();
        assert!(env.sig.is_none(), "fake signs in open posture");

        let header = serde_json::to_string(&env).unwrap();
        let parsed: Envelope = serde_json::from_str(&header).unwrap();
        assert!(
            matches!(gw.verify_envelope(&parsed).await, Assurance::Anonymous { .. }),
            "anonymous assurance round-trips through the wire format"
        );
    }

    #[tokio::test]
    async fn fake_vault_records_stores_through_dyn_port() {
        let fake = Arc::new(FakeKoiGateway::new(active_pond_status()));
        let gw: Arc<dyn KoiGateway> = fake.clone();

        gw.vault_store("borrowed:mongo:credentials", "s3cret")
            .unwrap();

        let recorded = fake.vault.lock().unwrap();
        assert_eq!(
            recorded.as_slice(),
            [("borrowed:mongo:credentials".to_string(), "s3cret".to_string())]
        );
    }

    #[tokio::test]
    async fn embedded_gateway_degrades_without_certmesh() {
        // All capabilities disabled — mirrors src/testing.rs construction.
        let handle = Arc::new(
            koi_embedded::Builder::new()
                .service_mode(koi_embedded::ServiceMode::EmbeddedOnly)
                .mdns(false)
                .dns_enabled(false)
                .health(false)
                .certmesh(false)
                .proxy(false)
                .udp(false)
                .http(false)
                .build()
                .expect("koi builder succeeds with all features disabled")
                .start()
                .await
                .expect("koi starts with all features disabled"),
        );

        let gw: Arc<dyn KoiGateway> = Arc::new(EmbeddedKoiGateway::new(handle));

        assert!(
            matches!(gw.pond_status().await, Err(GatewayError::Unavailable(_))),
            "status degrades to Unavailable without certmesh"
        );
        assert!(gw.ca_cert_path().is_none());
        assert!(gw.vault_store("k", "v").is_err());

        // Event subscription still works — just carries no pond events.
        let mut rx = gw.subscribe_events();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn koi_event_mapping_filters_to_pond_variants() {
        use koi_embedded::KoiEvent;
        use chrono::TimeZone;

        let renewed = map_koi_event(KoiEvent::CertRenewed {
            expires_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        });
        assert!(matches!(renewed, Some(GatewayEvent::CertRenewed { .. })));

        let failed = map_koi_event(KoiEvent::CertRenewalFailed {
            reason: "ca unreachable".into(),
            consecutive_failures: 2,
        });
        assert!(matches!(
            failed,
            Some(GatewayEvent::RenewalFailed { consecutive_failures: 2, .. })
        ));

        // Non-pond events are dropped at the source.
        assert!(map_koi_event(KoiEvent::CertmeshDestroyed).is_none());
    }
}
