//! `MockTransport` — a scripted event source for integration tests.
//!
//! Simulates moss without requiring an HTTP server. Tests queue events
//! via [`MockTransport::queue`] (before `run`) or via the cloneable
//! handle returned by [`MockTransport::handle`] (during `run`). The
//! transport drains the queue into Pulse on a short internal tick.

use crate::garden::{BoxFuture, Event, Pulse, Transport};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Kinds emitted by `MockTransport` by default. Covers the full set of
/// canonical `core.*` kinds shipped through Books I–VII; integration
/// tests can publish any of them without needing to register
/// namespaces manually.
pub const MOCK_EMITTED_KINDS: &[&str] = &[
    "core.presence.snapshot",
    "core.stone.health.changed",
    "core.stone.load.updated",
    "core.stone.tended",
    "core.service.started",
    "core.service.stopped",
    "core.storage.connected",
    "core.storage.detected",
    "core.storage.removed",
    "core.command.invocation",
    "core.command.result",
    "core.garden.snapshot",
];

/// Interval at which the mock transport drains its queued events into
/// Pulse. Small enough that tests don't need to sleep long to observe
/// delivery.
const MOCK_TICK: Duration = Duration::from_millis(5);

/// Handle to a MockTransport that can queue events before or during
/// `run`. Cheaply cloneable; all clones share the same queue.
#[derive(Clone)]
pub struct MockTransportHandle {
    queue: Arc<Mutex<VecDeque<Event>>>,
}

impl MockTransportHandle {
    /// Queue an event for the next tick.
    pub fn queue(&self, event: Event) {
        self.queue
            .lock()
            .expect("mock transport queue poisoned")
            .push_back(event);
    }

    /// Queue multiple events in order.
    pub fn queue_all<I: IntoIterator<Item = Event>>(&self, events: I) {
        let mut q = self
            .queue
            .lock()
            .expect("mock transport queue poisoned");
        q.extend(events);
    }

    /// Number of events still waiting in the queue (useful for test
    /// assertions about backpressure).
    pub fn pending(&self) -> usize {
        self.queue
            .lock()
            .expect("mock transport queue poisoned")
            .len()
    }
}

/// Scripted event source for integration tests.
pub struct MockTransport {
    queue: Arc<Mutex<VecDeque<Event>>>,
    emitted_kinds: &'static [&'static str],
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTransport {
    /// Construct with the default kind set ([`MOCK_EMITTED_KINDS`]).
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            emitted_kinds: MOCK_EMITTED_KINDS,
        }
    }

    /// Override the emitted kinds list (must be `&'static` — Transport
    /// trait requirement). Useful when a test needs a namespace the
    /// default set doesn't cover.
    pub fn with_emitted_kinds(mut self, kinds: &'static [&'static str]) -> Self {
        self.emitted_kinds = kinds;
        self
    }

    /// Queue an event immediately (before `run` starts). Equivalent to
    /// `self.handle().queue(event)`.
    pub fn queue(&self, event: Event) {
        self.queue
            .lock()
            .expect("mock transport queue poisoned")
            .push_back(event);
    }

    /// Get a cloneable handle for queuing events during `run`.
    pub fn handle(&self) -> MockTransportHandle {
        MockTransportHandle {
            queue: self.queue.clone(),
        }
    }
}

impl Transport for MockTransport {
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let queue = self.queue.clone();
        Box::pin(async move {
            let mut interval = tokio::time::interval(MOCK_TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await; // consume immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let batch: Vec<Event> = {
                            let mut q = queue.lock().expect("queue poisoned");
                            q.drain(..).collect()
                        };
                        for event in batch {
                            let _ = pulse.ingest(event);
                        }
                    }
                    _ = shutdown.cancelled() => {
                        // Final drain before exit.
                        let batch: Vec<Event> = {
                            let mut q = queue.lock().expect("queue poisoned");
                            q.drain(..).collect()
                        };
                        for event in batch {
                            let _ = pulse.ingest(event);
                        }
                        break;
                    }
                }
            }
        })
    }

    fn emitted_kinds(&self) -> &'static [&'static str] {
        self.emitted_kinds
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{Event, EventPayload, PulseConfig};
    use std::any::Any;

    #[derive(Debug)]
    struct Tended;
    impl EventPayload for Tended {
        const KIND: &'static str = "core.stone.tended";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn core_pulse() -> Arc<Pulse> {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 16,
            broadcast_capacity: 64,
        }));
        pulse.register_namespace("core");
        pulse
    }

    #[test]
    fn new_covers_all_core_kinds() {
        let t = MockTransport::new();
        let kinds = t.emitted_kinds();
        assert!(kinds.contains(&"core.presence.snapshot"));
        assert!(kinds.contains(&"core.stone.tended"));
        assert!(kinds.contains(&"core.command.invocation"));
    }

    #[test]
    fn handle_and_queue_share_state() {
        let t = MockTransport::new();
        let handle = t.handle();

        t.queue(Event::new(Tended));
        handle.queue(Event::new(Tended));

        assert_eq!(handle.pending(), 2);
    }

    #[tokio::test]
    async fn queued_events_reach_pulse_after_run_starts() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let transport = MockTransport::new();
        transport.queue(Event::new(Tended));
        transport.queue(Event::new(Tended));

        let shutdown = CancellationToken::new();
        let sh = shutdown.clone();
        let p = pulse.clone();
        let handle = tokio::spawn(async move {
            (Box::new(transport) as Box<dyn Transport>).run(p, sh).await;
        });

        // Give the transport a couple of ticks to drain.
        tokio::time::sleep(Duration::from_millis(30)).await;

        let first = rx.try_recv().unwrap();
        let second = rx.try_recv().unwrap();
        assert_eq!(first.kind, "core.stone.tended");
        assert_eq!(second.kind, "core.stone.tended");

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn events_queued_during_run_are_delivered() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let transport = MockTransport::new();
        let handle = transport.handle();

        let shutdown = CancellationToken::new();
        let sh = shutdown.clone();
        let p = pulse.clone();
        let run_handle = tokio::spawn(async move {
            (Box::new(transport) as Box<dyn Transport>).run(p, sh).await;
        });

        // Queue after run started.
        tokio::time::sleep(Duration::from_millis(10)).await;
        handle.queue(Event::new(Tended));

        tokio::time::sleep(Duration::from_millis(20)).await;
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.stone.tended");

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(1), run_handle).await;
    }

    #[tokio::test]
    async fn exits_cleanly_on_shutdown() {
        let pulse = core_pulse();
        let transport = MockTransport::new();
        let shutdown = CancellationToken::new();
        let sh = shutdown.clone();
        let handle = tokio::spawn(async move {
            (Box::new(transport) as Box<dyn Transport>).run(pulse, sh).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        shutdown.cancel();

        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("did not exit in 1s")
            .expect("panicked");
    }
}
