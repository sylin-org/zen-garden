//! `TestHarness` — compact wrapper around [`Companion`] for integration tests.
//!
//! Separates the builder phase ([`TestHarness`]) from the running phase
//! ([`RunningHarness`]). Tests typically:
//!
//! 1. Construct `TestHarness::new(name)`.
//! 2. Add transports / factories with the fluent `with_*` methods.
//! 3. Call `.start().await` to produce a `RunningHarness`.
//! 4. Publish events, query garden state, assert on recorded outcomes.
//! 5. Call `.shutdown().await` to complete cleanly.
//!
//! The companion's Pulse and Garden are exposed before starting so tests
//! can register custom namespaces or pre-seed state.

use crate::adapters::{AdapterFactory, Adapters};
use crate::companion::Companion;
use crate::garden::{Event, EventPayload, Garden, IngestResult, Pulse, Transport};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Test-scope wrapper around [`Companion`] — pre-start phase.
pub struct TestHarness {
    companion: Companion,
}

impl TestHarness {
    /// Construct a harness with default configuration. A short flush
    /// interval (10ms) is used so tests don't need to wait the
    /// production default of 50ms.
    pub fn new(name: impl Into<String>) -> Self {
        let companion = Companion::new(name).with_flush_interval(Duration::from_millis(10));
        Self { companion }
    }

    /// Override the coalesced-event flush interval.
    pub fn with_flush_interval(mut self, d: Duration) -> Self {
        self.companion = self.companion.with_flush_interval(d);
        self
    }

    /// Attach a transport (real or mock).
    pub fn with_transport<T: Transport>(mut self, t: T) -> Self {
        self.companion = self.companion.with_transport(t);
        self
    }

    /// Register an adapter factory.
    pub fn with_adapter_factory<F: AdapterFactory>(mut self, f: F) -> Self {
        self.companion = self.companion.with_adapter_factory(f);
        self
    }

    /// Access the shared Pulse before starting — useful for registering
    /// extra namespaces or inspecting metrics.
    pub fn pulse(&self) -> Arc<Pulse> {
        self.companion.pulse()
    }

    /// Access the Garden before starting.
    pub fn garden(&self) -> Arc<Garden> {
        self.companion.garden()
    }

    /// Access the adapter supervisor before starting — useful for
    /// registering factories programmatically.
    pub fn adapters(&self) -> Arc<Adapters> {
        self.companion.adapters()
    }

    /// Start the companion. Returns a `RunningHarness` whose shutdown
    /// handle must be awaited to ensure clean exit.
    pub async fn start(self) -> RunningHarness {
        let pulse = self.companion.pulse();
        let garden = self.companion.garden();
        let adapters = self.companion.adapters();
        let shutdown = self.companion.shutdown_token();

        let run_handle = tokio::spawn(async move {
            self.companion.run().await
        });

        // Brief pause so background tasks (flush timer, Garden projection,
        // supervisor, transports) spin up before the first test assertion.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        RunningHarness {
            pulse,
            garden,
            adapters,
            shutdown,
            run_handle,
        }
    }
}

/// Test-scope wrapper — running phase.
pub struct RunningHarness {
    pulse: Arc<Pulse>,
    garden: Arc<Garden>,
    adapters: Arc<Adapters>,
    shutdown: CancellationToken,
    run_handle: JoinHandle<anyhow::Result<()>>,
}

impl RunningHarness {
    pub fn pulse(&self) -> Arc<Pulse> {
        self.pulse.clone()
    }

    pub fn garden(&self) -> Arc<Garden> {
        self.garden.clone()
    }

    pub fn adapters(&self) -> Arc<Adapters> {
        self.adapters.clone()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Publish an event to the running companion's Pulse, bypassing
    /// transport simulation. Useful for scripted scenarios that don't
    /// care about transport behaviour.
    pub fn publish<P: EventPayload>(&self, payload: P) -> IngestResult {
        self.pulse.ingest(Event::new(payload))
    }

    /// Wait up to `timeout` for Garden to report ready (i.e. a
    /// `PresenceSnapshot` has been projected). Returns `true` on ready,
    /// `false` on timeout.
    pub async fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self.garden.is_ready() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Cancel the shutdown token and await the companion's `run` task.
    /// Bounded timeout (5 seconds) — returns the companion's result
    /// if it exited within the window, or `None` if it timed out.
    pub async fn shutdown(self) -> Option<anyhow::Result<()>> {
        self.shutdown.cancel();
        match tokio::time::timeout(Duration::from_secs(5), self.run_handle).await {
            Ok(Ok(result)) => Some(result),
            Ok(Err(_join_err)) => Some(Err(anyhow::anyhow!("companion task panicked"))),
            Err(_timeout) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{EventPayload, Pulse};
    use crate::testing::{MockTransport, RecordingAdapter, recording_adapter::RecordingHandleExt};
    use std::any::Any;

    #[derive(Debug)]
    struct Tended;
    impl EventPayload for Tended {
        const KIND: &'static str = "core.stone.tended";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[tokio::test]
    async fn harness_builder_preserves_pulse_and_garden() {
        let h = TestHarness::new("x");
        let _p: Arc<Pulse> = h.pulse();
        let _g: Arc<Garden> = h.garden();
    }

    #[tokio::test]
    async fn start_produces_running_harness_and_shutdown_is_clean() {
        let h = TestHarness::new("x").start().await;
        let result = h.shutdown().await;
        assert!(result.is_some(), "shutdown timed out");
        assert!(result.unwrap().is_ok(), "companion returned error");
    }

    #[tokio::test]
    async fn publish_delivers_events_to_recording_adapter() {
        let (records, factory) = RecordingAdapter::factory(
            "test.record",
            "only",
            &["core.stone.tended"],
        );
        let h = TestHarness::new("x")
            .with_transport(MockTransport::new())
            .with_adapter_factory(factory)
            .start()
            .await;

        // Give supervisor a discovery tick to spawn the adapter.
        tokio::time::sleep(Duration::from_millis(100)).await;

        h.publish(Tended);
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!records.is_empty(), "adapter did not record the event");
        assert_eq!(records.kinds()[0], "core.stone.tended");

        let _ = h.shutdown().await;
    }
}
