//! Chirp Listener - UDP topology broadcast on lifecycle events
//!
//! Listens for offering lifecycle events that change the garden topology
//! and triggers UDP chirp announcements to notify other stones.
//!
//! Debouncing is per (event_type, offering_id) pair:
//! - Same event type + same offering = debounced
//! - Different event type + same offering = passes
//! - Different offering = passes

use crate::domain::events::DomainEvent;
use crate::infra::event_bus::EventListener;
use garden_common::infra::StringPairDebouncer;
use std::sync::Arc;
use std::time::Duration;

/// Default debounce duration for chirp events
pub const DEFAULT_CHIRP_DEBOUNCE_MS: u64 = 500;

/// Callback type for triggering chirp announcements
pub type ChirpCallback = Arc<dyn Fn() + Send + Sync>;

/// Listener that triggers chirp announcements on topology changes
///
/// Uses shared debouncer from garden_common per (event_type, offering_id) pair:
/// - Multiple "deployed" events for the same offering within the window = debounced
/// - A "deployed" followed by "updated" for the same offering = both pass
/// - Events for different offerings = both pass
pub struct ChirpListener {
    /// Callback to trigger chirp (avoids circular dependency with Moss)
    chirp_fn: ChirpCallback,
    /// Debouncer for rate limiting
    debouncer: StringPairDebouncer,
}

impl ChirpListener {
    /// Create a new chirp listener with the given callback and default debounce
    ///
    /// The callback should sync services and send a chirp announcement.
    /// Typically this wraps `announce_if_changed()` or similar.
    pub fn new(chirp_fn: ChirpCallback) -> Self {
        Self::with_debounce(chirp_fn, Duration::from_millis(DEFAULT_CHIRP_DEBOUNCE_MS))
    }

    /// Create a new chirp listener with custom debounce duration
    pub fn with_debounce(chirp_fn: ChirpCallback, debounce: Duration) -> Self {
        Self {
            chirp_fn,
            debouncer: StringPairDebouncer::new(debounce),
        }
    }
}

impl EventListener for ChirpListener {
    async fn on_event(&self, event: &DomainEvent) {
        // Only process offering events for chirp (topology changes)
        let offering_event = match event {
            DomainEvent::Offering(e) => e,
            // Storage and Stone events don't affect garden topology
            _ => return,
        };

        // Only chirp for topology-changing events
        if !offering_event.should_chirp() {
            tracing::trace!(
                event_type = offering_event.event_type(),
                "Skipping chirp for non-topology event"
            );
            return;
        }

        // Check debounce per (event_type, offering_id)
        if !self
            .debouncer
            .should_pass_str(offering_event.event_type(), offering_event.offering_id())
        {
            tracing::trace!(
                event_type = offering_event.event_type(),
                offering_id = offering_event.offering_id(),
                "Debouncing chirp (same event+offering within window)"
            );
            return;
        }

        tracing::debug!(
            event_type = offering_event.event_type(),
            offering = offering_event.name(),
            "Triggering chirp announcement"
        );

        // Call the chirp function
        (self.chirp_fn)();
    }

    fn name(&self) -> &'static str {
        super::names::CHIRP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::OfferingEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;

    #[tokio::test]
    async fn test_chirp_on_deploy() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::new(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));

        let event = DomainEvent::Offering(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
        listener.on_event(&event).await;

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_no_chirp_on_start() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::new(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));

        // Start/stop don't trigger chirps (by design - topology doesn't change)
        let event = DomainEvent::Offering(OfferingEvent::started("id-1", "mongodb", "stone-01"));
        listener.on_event(&event).await;

        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_no_chirp_on_storage_events() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::new(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));

        // Storage events don't trigger chirps
        let event = DomainEvent::Storage(crate::domain::events::StorageEvent::storage_connected(
            "backup",
            "/dev/sdb1",
            "/mnt/backup",
            500,
            vec!["seed-bank".to_string()],
        ));
        listener.on_event(&event).await;

        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_debounce_same_event_same_offering() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::with_debounce(
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(100),
        );

        // Same event type + same offering = debounced
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "healthy",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "degraded",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "healthy",
            )))
            .await;

        // Only first event should have triggered chirp
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_different_event_types_not_debounced() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::with_debounce(
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(100),
        );

        // Different event types for same offering = all pass
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::updated(
                "id-1", "mongodb", "stone-01", "mongo:6", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "healthy",
            )))
            .await;

        // All three should trigger (different event types)
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_different_offerings_not_debounced() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::with_debounce(
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(100),
        );

        // Same event type but different offerings = all pass
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-2", "redis", "stone-01", "redis:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-3", "postgres", "stone-01", "pg:16",
            )))
            .await;

        // All three should trigger (different offerings)
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_debounce_window_expires() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();

        let listener = ChirpListener::with_debounce(
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(50),
        );

        // First event
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "healthy",
            )))
            .await;
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // Wait for debounce window to expire
        sleep(Duration::from_millis(100));

        // Same event should now trigger again
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::health_changed(
                "id-1", "mongodb", "stone-01", "healthy",
            )))
            .await;
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }
}
