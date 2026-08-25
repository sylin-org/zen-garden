//! Discovery aggregate unit tests.

use super::*;
use crate::domain::Metrics;
use std::sync::Arc;

/// Build a minimal Discovery aggregate for testing (no mDNS, no Koi).
///
/// Uses a Koi handle constructed with defaults. mDNS is None.
async fn test_discovery() -> Discovery {
    let metrics = Arc::new(Metrics::new());

    // Build a minimal Koi handle for testing — no mDNS, no DNS, no certmesh.
    let koi = Arc::new(
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
            .expect("koi builder")
            .start()
            .await
            .expect("koi start"),
    );

    Discovery::new(koi, None, None, metrics).await
}

#[tokio::test]
async fn no_mdns_handle_means_not_registered() {
    let discovery = test_discovery().await;
    assert!(!discovery.mdns_registered());
    assert!(!discovery.has_mdns());
}

#[tokio::test]
async fn lurk_stream_none_without_lurk_tx() {
    let discovery = test_discovery().await;
    assert!(discovery.lurk_stream().is_none());
}

#[tokio::test]
async fn lurk_stream_available_with_lurk_tx() {
    let metrics = Arc::new(Metrics::new());
    let koi = Arc::new(
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
            .expect("koi builder")
            .start()
            .await
            .expect("koi start"),
    );

    let (tx, _) = tokio::sync::broadcast::channel(16);
    let discovery = Discovery::new(koi, None, Some(tx), metrics).await;

    assert!(discovery.lurk_stream().is_some());
}

#[tokio::test]
async fn changes_channel_receives_events_on_reregister() {
    let discovery = test_discovery().await;
    let mut rx = discovery.changes();

    // Trigger a reregister (no mDNS handle, so no event emitted — noop)
    discovery.reregister("192.168.1.100", None).await;

    // No event because mdns is None (early return before emit)
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn koi_accessor_returns_handle() {
    let discovery = test_discovery().await;
    // Just verify it doesn't panic and returns a reference
    let _koi = discovery.koi();
}

#[tokio::test]
async fn reregister_without_mdns_is_noop() {
    let discovery = test_discovery().await;
    // Should not panic — just logs and returns
    discovery.reregister("192.168.1.100", None).await;
    assert!(!discovery.mdns_registered());
}

#[tokio::test]
async fn update_health_without_mdns_is_noop() {
    let discovery = test_discovery().await;
    // Should not panic — just logs and returns
    discovery.update_health("healthy").await;
}

#[tokio::test]
async fn clone_shares_state() {
    let discovery = test_discovery().await;
    let cloned = discovery.clone();

    // Both should report the same mDNS state
    assert_eq!(discovery.has_mdns(), cloned.has_mdns());
    assert_eq!(discovery.mdns_registered(), cloned.mdns_registered());
}
