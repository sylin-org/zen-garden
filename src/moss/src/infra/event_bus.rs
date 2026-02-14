//! Event Bus - Unified event dispatch for domain events
//!
//! Dispatches DomainEvent to registered listeners:
//! - ChirpListener: Broadcasts topology changes via UDP
//! - SseListener: Streams events to connected clients (Firefly, Cricket)
//! - TimerListener: Manages nurturing schedule timers
//!
//! Uses tokio broadcast channel for fan-out delivery.

use crate::domain::events::DomainEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Channel capacity for event broadcast
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Event bus for domain events
///
/// Provides a unified dispatch mechanism for all domain changes:
/// - Offering lifecycle (deploy, start, stop, etc.)
/// - Storage events (seed bank detection, removal)
/// - Stone events (tended, health changes)
///
/// Listeners subscribe via the receiver and handle events asynchronously.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Emit a domain event to all listeners
    ///
    /// Returns the number of receivers that received the event.
    /// A return of 0 means no listeners are currently subscribed.
    pub fn emit(&self, event: impl Into<DomainEvent>) {
        let event = event.into();
        let event_type = event.event_type().to_string();

        match self.sender.send(event) {
            Ok(count) => {
                tracing::debug!(event_type, receivers = count, "Event emitted");
            }
            Err(_) => {
                // No receivers - this is fine, just means no listeners are active
                tracing::trace!(event_type, "Event emitted (no receivers)");
            }
        }
    }

    /// Subscribe to events
    ///
    /// Returns a receiver that will receive all future events.
    /// Past events are not replayed.
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }

    /// Get the current number of receivers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Listener trait for event consumers
///
/// Implement this trait to create custom event listeners.
/// The listener runs in its own task and processes events asynchronously.
#[async_trait::async_trait]
pub trait EventListener: Send + Sync + 'static {
    /// Handle an incoming event
    ///
    /// Called for each event received. Should not block for long periods.
    async fn on_event(&self, event: &DomainEvent);

    /// Listener name for logging
    fn name(&self) -> &'static str;
}

/// Spawn a listener task that processes events from the bus
///
/// The task runs until the receiver is dropped or an error occurs.
/// Returns a handle that can be used to abort the task.
pub fn spawn_listener<L: EventListener>(
    bus: &EventBus,
    listener: Arc<L>,
) -> tokio::task::JoinHandle<()> {
    let mut receiver = bus.subscribe();
    let listener_name = listener.name();

    tokio::spawn(async move {
        tracing::info!(listener = listener_name, "Event listener started");

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let event_type = event.event_type();
                    tracing::trace!(listener = listener_name, event_type, "Processing event");
                    listener.on_event(&event).await;
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    tracing::warn!(
                        listener = listener_name,
                        skipped = count,
                        "Listener lagged behind, events skipped"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        listener = listener_name,
                        "Event bus closed, listener stopping"
                    );
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::{OfferingEvent, StoneEvent, StorageEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, Duration};

    struct CountingListener {
        count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl EventListener for CountingListener {
        async fn on_event(&self, _event: &DomainEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn name(&self) -> &'static str {
            "counting"
        }
    }

    #[tokio::test]
    async fn test_event_bus_basic() {
        let bus = EventBus::new();
        let listener = Arc::new(CountingListener {
            count: AtomicUsize::new(0),
        });

        let _handle = spawn_listener(&bus, listener.clone());

        // Give listener time to start
        sleep(Duration::from_millis(10)).await;

        // Emit some events
        bus.emit(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
        bus.emit(OfferingEvent::started("id-1", "mongodb", "stone-01"));
        bus.emit(OfferingEvent::stopped("id-1", "mongodb", "stone-01"));

        // Give listener time to process
        sleep(Duration::from_millis(50)).await;

        assert_eq!(listener.count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_multiple_listeners() {
        let bus = EventBus::new();

        let listener1 = Arc::new(CountingListener {
            count: AtomicUsize::new(0),
        });
        let listener2 = Arc::new(CountingListener {
            count: AtomicUsize::new(0),
        });

        let _h1 = spawn_listener(&bus, listener1.clone());
        let _h2 = spawn_listener(&bus, listener2.clone());

        sleep(Duration::from_millis(10)).await;

        bus.emit(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));

        sleep(Duration::from_millis(50)).await;

        // Both listeners should receive the event
        assert_eq!(listener1.count.load(Ordering::SeqCst), 1);
        assert_eq!(listener2.count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_storage_events() {
        let bus = EventBus::new();
        let listener = Arc::new(CountingListener {
            count: AtomicUsize::new(0),
        });

        let _handle = spawn_listener(&bus, listener.clone());
        sleep(Duration::from_millis(10)).await;

        bus.emit(StorageEvent::seed_bank_detected(
            "backup",
            "/dev/sdb1",
            "/mnt/backup",
            500,
        ));
        bus.emit(StorageEvent::seed_bank_removed("backup", "/dev/sdb1"));

        sleep(Duration::from_millis(50)).await;

        assert_eq!(listener.count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_stone_events() {
        let bus = EventBus::new();
        let listener = Arc::new(CountingListener {
            count: AtomicUsize::new(0),
        });

        let _handle = spawn_listener(&bus, listener.clone());
        sleep(Duration::from_millis(10)).await;

        bus.emit(StoneEvent::tended("rake", "leo-laptop", None));
        bus.emit(StoneEvent::health_changed("thriving", 25.0, 40.0));

        sleep(Duration::from_millis(50)).await;

        assert_eq!(listener.count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_no_receivers_ok() {
        let bus = EventBus::new();
        // Should not panic with no receivers
        bus.emit(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
    }
}
