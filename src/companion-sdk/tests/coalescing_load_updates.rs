//! Scenario: rapid-fire `LoadUpdated` events coalesce to a single delivery
//! per flush window.
//!
//! Publishes 100 `core.stone.load.updated` events in quick succession.
//! Because `StoneLoadUpdatedPayload::COALESCING == true`, Pulse buffers
//! them; Companion's flush timer (10ms in the harness) delivers only
//! the latest value per flush cycle. Expectation: RecordingAdapter
//! receives strictly fewer than 100 events, and the last one carries
//! the last-published CPU percent.

use garden_common::presence::StoneLoadUpdatedPayload;
use garden_companion_sdk::testing::{
    MockTransport, RecordingAdapter, TestHarness, recording_adapter::RecordingHandleExt,
};
use garden_companion_sdk::garden::{Event, StoneLoadUpdatedExt};
use std::time::Duration;

const LOAD_SUBS: &[&str] = &["core.stone.load.updated"];

#[tokio::test]
async fn burst_of_load_updates_coalesces_before_delivery() {
    let transport = MockTransport::new();
    let handle = transport.handle();

    let (records, factory) = RecordingAdapter::factory("test.load-record", "only", LOAD_SUBS);

    let harness = TestHarness::new("scenario-coalescing")
        .with_flush_interval(Duration::from_millis(20))
        .with_transport(transport)
        .with_adapter_factory(factory)
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish 100 events with varying CPU so we can check which "won".
    for i in 0..100u8 {
        handle.queue(Event::new(StoneLoadUpdatedPayload {
            cpu_percent: i as f64,
            memory_percent: 40.0,
            disk_percent: 30.0,
            io_percent: 0.0,
            gpu_percent: 0.0,
            gpu_active: false,
            net_rx_bytes_per_sec: 0,
            net_tx_bytes_per_sec: 0,
        }));
    }

    // Wait well past the flush interval so any pending events drain.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let count = records.len();
    assert!(count > 0, "no coalesced events delivered");
    assert!(
        count < 100,
        "expected coalesced delivery (< 100); got {count}"
    );

    // The final delivered event should carry the highest CPU (99) since
    // coalescing keeps "latest per kind".
    let last_cpu = records
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| e.payload::<StoneLoadUpdatedPayload>())
        .map(|p| p.load_domain().cpu.as_u8())
        .max()
        .expect("at least one event");
    assert_eq!(last_cpu, 99, "latest value should win coalescing");

    let _ = harness.shutdown().await;
}
