//! `RecordingAdapter` — an adapter that records every event it receives.
//!
//! The workhorse of companion integration tests: register it via
//! [`RecordingAdapter::factory`], run the scenario, assert on the
//! returned records handle.

use crate::adapters::{
    Adapter, AdapterInfo, AdapterProfile, DeliveryPolicy,
    adapter::BoxFuture,
};
use crate::garden::{Event, Pulse};
use crate::moss_client::MossLocalClient;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Shared handle to a recording adapter's event log.
///
/// Cheaply cloneable. All clones share the same underlying `Vec<Event>`
/// — test code can hold one handle while the adapter holds another.
pub type RecordingHandle = Arc<Mutex<Vec<Event>>>;

/// Adapter that records every event it receives into a shared buffer.
///
/// Use [`RecordingAdapter::factory`] in most tests — it returns both a
/// ready-to-register `FakeFactory` and the inspection handle.
pub struct RecordingAdapter {
    info: AdapterInfo,
    profile: AdapterProfile,
    records: RecordingHandle,
}

impl RecordingAdapter {
    /// Construct with a fresh, empty records buffer. Returns
    /// `(adapter, handle)` — the handle can be cloned and held by test
    /// code while the adapter is consumed by the supervisor.
    pub fn new(
        kind: &'static str,
        id: impl Into<String>,
        subscriptions: &'static [&'static str],
    ) -> (Self, RecordingHandle) {
        let records: RecordingHandle = Arc::new(Mutex::new(Vec::new()));
        let adapter = Self {
            info: AdapterInfo {
                kind,
                id: id.into(),
                device: None,
            },
            profile: AdapterProfile {
                subscriptions,
                delivery: DeliveryPolicy::All,
                persisted_state: false,
            },
            records: records.clone(),
        };
        (adapter, records)
    }

    /// Construct with an externally-provided records buffer. Useful
    /// when the same buffer is shared across multiple adapter instances
    /// (rare, but supported).
    pub fn with_records(
        kind: &'static str,
        id: impl Into<String>,
        subscriptions: &'static [&'static str],
        records: RecordingHandle,
    ) -> Self {
        Self {
            info: AdapterInfo {
                kind,
                id: id.into(),
                device: None,
            },
            profile: AdapterProfile {
                subscriptions,
                delivery: DeliveryPolicy::All,
                persisted_state: false,
            },
            records,
        }
    }

    /// Build a [`FakeFactory`] that produces a RecordingAdapter with
    /// the given parameters. Returns `(records_handle, factory)` —
    /// pass the factory to [`crate::Companion::with_adapter_factory`]
    /// or [`crate::testing::TestHarness::with_adapter_factory`], then
    /// inspect `records_handle` after the scenario.
    pub fn factory(
        kind: &'static str,
        id: impl Into<String>,
        subscriptions: &'static [&'static str],
    ) -> (RecordingHandle, super::fake_factory::FakeFactory) {
        let id: String = id.into();
        let records: RecordingHandle = Arc::new(Mutex::new(Vec::new()));
        let records_for_factory = records.clone();
        let factory = super::fake_factory::FakeFactory::new(kind, move || {
            Box::new(RecordingAdapter::with_records(
                kind,
                id.clone(),
                subscriptions,
                records_for_factory.clone(),
            ))
        });
        (records, factory)
    }
}

impl Adapter for RecordingAdapter {
    fn info(&self) -> AdapterInfo {
        self.info.clone()
    }

    fn profile(&self) -> AdapterProfile {
        self.profile.clone()
    }

    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        _moss: Arc<MossLocalClient>,
        _pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let records = self.records.clone();
        Box::pin(async move {
            loop {
                tokio::select! {
                    maybe = events.recv() => match maybe {
                        Some(event) => {
                            records
                                .lock()
                                .expect("records lock poisoned")
                                .push(event);
                        }
                        None => break,
                    },
                    _ = shutdown.cancelled() => break,
                }
            }
        })
    }
}

/// Helpers for asserting on a recording handle.
pub trait RecordingHandleExt {
    /// Number of events recorded so far.
    fn len(&self) -> usize;

    /// True if no events recorded.
    fn is_empty(&self) -> bool;

    /// All recorded event kinds, in order of receipt.
    fn kinds(&self) -> Vec<&'static str>;
}

impl RecordingHandleExt for RecordingHandle {
    fn len(&self) -> usize {
        self.lock().expect("records lock poisoned").len()
    }

    fn is_empty(&self) -> bool {
        self.lock().expect("records lock poisoned").is_empty()
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.lock()
            .expect("records lock poisoned")
            .iter()
            .map(|e| e.kind)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{Event, EventPayload};
    use std::any::Any;

    #[derive(Debug)]
    struct Probe;
    impl EventPayload for Probe {
        const KIND: &'static str = "core.test.probe";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn new_returns_adapter_and_handle_sharing_state() {
        let (adapter, handle) = RecordingAdapter::new("test.x", "only", &["core.test.probe"]);
        assert_eq!(adapter.info().kind, "test.x");
        assert!(handle.is_empty());
    }

    #[test]
    fn factory_returns_handle_and_usable_factory() {
        use crate::adapters::AdapterFactory;
        let (handle, factory) = RecordingAdapter::factory(
            "test.x",
            "only",
            &["core.test.probe"],
        );
        let adapters = factory.discover();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].info().kind, "test.x");
        assert_eq!(adapters[0].info().id, "only");
        assert!(handle.is_empty());
    }

    #[tokio::test]
    async fn adapter_records_events_pushed_into_its_mpsc() {
        let (adapter, handle) = RecordingAdapter::new(
            "test.x",
            "only",
            &["core.test.probe"],
        );
        let (tx, rx) = mpsc::channel::<Event>(8);
        let shutdown = CancellationToken::new();
        let pulse = Arc::new(crate::garden::Pulse::with_defaults());
        let moss = Arc::new(MossLocalClient::new("http://127.0.0.1:0"));

        let sh = shutdown.clone();
        let run_handle = tokio::spawn(async move {
            Box::new(adapter).run(rx, moss, pulse, sh).await;
        });

        tx.send(Event::new(Probe)).await.unwrap();
        tx.send(Event::new(Probe)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        assert_eq!(handle.len(), 2);
        assert_eq!(handle.kinds(), vec!["core.test.probe", "core.test.probe"]);

        shutdown.cancel();
        drop(tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), run_handle).await;
    }

    #[tokio::test]
    async fn adapter_exits_on_shutdown_even_with_live_sender() {
        let (adapter, handle) = RecordingAdapter::new(
            "test.x",
            "only",
            &["core.test.probe"],
        );
        let (tx, rx) = mpsc::channel::<Event>(1);
        let shutdown = CancellationToken::new();
        let pulse = Arc::new(crate::garden::Pulse::with_defaults());
        let moss = Arc::new(MossLocalClient::new("http://127.0.0.1:0"));

        let sh = shutdown.clone();
        let run_handle = tokio::spawn(async move {
            Box::new(adapter).run(rx, moss, pulse, sh).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        shutdown.cancel();

        // Should exit even though tx is alive (no events pending).
        tokio::time::timeout(std::time::Duration::from_secs(1), run_handle)
            .await
            .expect("did not exit in 1s")
            .expect("panicked");

        assert!(handle.is_empty());
        drop(tx);
    }

    #[test]
    fn handle_ext_kinds_returns_recorded_kinds_in_order() {
        let handle: RecordingHandle = Arc::new(Mutex::new(Vec::new()));
        handle
            .lock()
            .unwrap()
            .push(Event::new(Probe));
        assert_eq!(handle.kinds(), vec!["core.test.probe"]);
        assert_eq!(handle.len(), 1);
        assert!(!handle.is_empty());
    }
}
