//! Scenario: SSE presence snapshot flows end-to-end.
//!
//! The harness publishes a `core.presence.snapshot` event into the Pulse
//! via `MockTransport`; the Garden projects it; a `RecordingAdapter`
//! subscribed to snapshots observes the event; property accessors on
//! `Garden` reflect the snapshot contents.

use garden_common::presence::{OfferingState, PresenceSnapshot, StoneState};
use garden_companion_sdk::testing::{
    MockTransport, RecordingAdapter, TestHarness, recording_adapter::RecordingHandleExt,
};
use garden_common::domain::{Health, Pond};
use garden_companion_sdk::garden::Event;
use std::time::Duration;

const SNAPSHOT_SUBS: &[&str] = &["core.presence.snapshot"];

fn sample_snapshot() -> PresenceSnapshot {
    PresenceSnapshot {
        stone: StoneState {
            name: "scenario-stone".into(),
            health: "thriving".into(),
            cpu_percent: 12.0,
            memory_percent: 48.0,
            disk_percent: 33.0,
            uptime_seconds: 1800,
            pond_active: true,
            io_percent: 5.0,
            gpu_percent: 0.0,
            net_rx_bytes_per_sec: 4096,
            net_tx_bytes_per_sec: 2048,
            has_gpu: false,
            gpu_active: false,
            is_lantern: false,
            has_cricket: true,
            hour: 9.75,
            seed_bank: None,
        },
        offerings: vec![OfferingState {
            name: "mongodb".into(),
            status: "running".into(),
            health: "healthy".into(),
        }],
        timestamp: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn presence_snapshot_projects_through_garden_and_reaches_adapter() {
    let transport = MockTransport::new();
    let handle = transport.handle();

    let (records, factory) =
        RecordingAdapter::factory("test.snapshot-record", "only", SNAPSHOT_SUBS);

    let harness = TestHarness::new("scenario-full-pipeline")
        .with_transport(transport)
        .with_adapter_factory(factory)
        .start()
        .await;

    // Allow the supervisor one discovery tick so the adapter is active.
    tokio::time::sleep(Duration::from_millis(100)).await;

    handle.queue(Event::new(sample_snapshot()));

    // Wait for Garden to reflect the projected state. Pulse tick is 5ms,
    // Garden projection is immediate on receive, but transport tick is
    // 5ms and first discovery tick might delay slightly.
    let ready = harness.wait_ready(Duration::from_secs(2)).await;
    assert!(ready, "Garden did not reach ready state in time");

    // Garden properties reflect the snapshot.
    let garden = harness.garden();
    assert_eq!(garden.stone_name().as_deref(), Some("scenario-stone"));
    assert_eq!(garden.health(), Health::Thriving);
    assert_eq!(garden.pond(), Pond::Member);
    assert_eq!(garden.offerings().len(), 1);
    assert_eq!(garden.offerings()[0].name, "mongodb");

    // Give the adapter's filter task a beat to forward the event.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !records.is_empty(),
        "adapter never received the presence.snapshot event"
    );
    assert_eq!(records.kinds()[0], "core.presence.snapshot");

    let _ = harness.shutdown().await;
}
