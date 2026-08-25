//! Scenario: subscription filter delivers only declared kinds.
//!
//! Two `RecordingAdapter`s with different `AdapterProfile::subscriptions`
//! share the same Pulse. Events of multiple kinds are published; each
//! adapter receives only events its subscription declares.

use garden_common::presence::{StoneHealthChangedPayload, StoneLoadUpdatedPayload};
use garden_companion_sdk::testing::{
    MockTransport, RecordingAdapter, TestHarness, recording_adapter::RecordingHandleExt,
};
use garden_companion_sdk::garden::Event;
use std::time::Duration;

const HEALTH_ONLY: &[&str] = &["core.stone.health.changed"];
const LOAD_ONLY: &[&str] = &["core.stone.load.updated"];

#[tokio::test]
async fn adapters_receive_only_their_subscribed_kinds() {
    let transport = MockTransport::new();
    let handle = transport.handle();

    let (health_records, health_factory) =
        RecordingAdapter::factory("test.health-only", "h", HEALTH_ONLY);
    let (load_records, load_factory) =
        RecordingAdapter::factory("test.load-only", "l", LOAD_ONLY);

    let harness = TestHarness::new("scenario-subscription-filtering")
        .with_flush_interval(Duration::from_millis(10))
        .with_transport(transport)
        .with_adapter_factory(health_factory)
        .with_adapter_factory(load_factory)
        .start()
        .await;

    // Allow both adapters to spawn.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a mix of the two kinds.
    handle.queue(Event::new(StoneHealthChangedPayload {
        health: "thriving".into(),
        cpu_percent: 0.0,
        memory_percent: 0.0,
    }));
    handle.queue(Event::new(StoneLoadUpdatedPayload {
        cpu_percent: 50.0,
        memory_percent: 40.0,
        disk_percent: 30.0,
        io_percent: 0.0,
        gpu_percent: 0.0,
        gpu_active: false,
        net_rx_bytes_per_sec: 0,
        net_tx_bytes_per_sec: 0,
    }));
    handle.queue(Event::new(StoneHealthChangedPayload {
        health: "withering".into(),
        cpu_percent: 0.0,
        memory_percent: 0.0,
    }));

    // Wait for the load event to flush out of the coalesce buffer and
    // both adapters to process their subscribed kinds.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Health adapter saw only the two health events.
    let health_seen = health_records.kinds();
    assert_eq!(
        health_seen,
        vec!["core.stone.health.changed", "core.stone.health.changed"],
        "health adapter received wrong set of kinds"
    );

    // Load adapter saw only the load event (coalesced to one).
    let load_seen = load_records.kinds();
    assert_eq!(
        load_seen,
        vec!["core.stone.load.updated"],
        "load adapter received wrong set of kinds"
    );

    let _ = harness.shutdown().await;
}
