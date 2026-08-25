//! Scenario: shutdown completeness.
//!
//! After a running harness is shut down, every background task —
//! Pulse flush timer, Garden projection, Adapters supervisor, attached
//! transport(s), and spawned adapter run-loops — exits cleanly within
//! the bounded join window. The companion's `run` future resolves with
//! `Ok(())`, and live adapters observe their cancellation token.

use garden_common::presence::StoneHealthChangedPayload;
use garden_companion_sdk::adapters::{
    Adapter, AdapterInfo, AdapterProfile, adapter::BoxFuture,
};
use garden_companion_sdk::testing::{FakeFactory, MockTransport, TestHarness};
use garden_companion_sdk::garden::{Event, Pulse};
use garden_companion_sdk::moss_client::MossLocalClient;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Adapter that flags when its run-loop starts and when it exits.
struct ObservableAdapter {
    id: String,
    started: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}

impl Adapter for ObservableAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "test.observable",
            id: self.id.clone(),
            device: None,
        }
    }
    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: &["core.stone.health.changed"],
            ..AdapterProfile::default()
        }
    }
    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        _g: Arc<MossLocalClient>,
        _p: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let started = self.started.clone();
        let exited = self.exited.clone();
        Box::pin(async move {
            started.store(true, Ordering::Relaxed);
            loop {
                tokio::select! {
                    maybe = events.recv() => {
                        if maybe.is_none() { break; }
                    }
                    _ = shutdown.cancelled() => break,
                }
            }
            exited.store(true, Ordering::Relaxed);
        })
    }
}

#[tokio::test]
async fn shutdown_reaps_all_background_tasks_cleanly() {
    let started = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));

    let started_clone = started.clone();
    let exited_clone = exited.clone();
    let factory = FakeFactory::new("test.observable", move || {
        Box::new(ObservableAdapter {
            id: "only".into(),
            started: started_clone.clone(),
            exited: exited_clone.clone(),
        })
    });

    let transport = MockTransport::new();
    let handle = transport.handle();

    let harness = TestHarness::new("scenario-shutdown-completeness")
        .with_flush_interval(Duration::from_millis(10))
        .with_transport(transport)
        .with_adapter_factory(factory)
        .start()
        .await;

    // Let the supervisor spawn the adapter.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(started.load(Ordering::Relaxed), "adapter never started");

    // Put some traffic through the pipeline so flush timer, projection,
    // and adapter filter task all have live work to exit from.
    handle.queue(Event::new(StoneHealthChangedPayload {
        health: "thriving".into(),
        cpu_percent: 10.0,
        memory_percent: 20.0,
    }));
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Shutdown: the companion's run future must resolve Ok within the
    // bounded 5s window and the adapter must observe cancellation.
    let result = harness.shutdown().await;
    assert!(result.is_some(), "companion did not exit within join window");
    assert!(
        result.unwrap().is_ok(),
        "companion run returned an error instead of clean exit"
    );
    assert!(
        exited.load(Ordering::Relaxed),
        "adapter run-loop never observed shutdown cancellation"
    );
}
