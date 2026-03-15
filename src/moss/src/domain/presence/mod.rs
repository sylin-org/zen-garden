//! Presence domain — election service and notification registry.

use std::sync::Arc;

/// Presence domain context (`state.presence`).
///
/// Groups the services that manage this stone's presence in the garden:
/// distributed elections and the notification registry (chirp tags).
#[derive(Clone)]
pub struct Presence {
    /// Distributed election service — leader election across garden stones.
    pub elections: Arc<crate::tasks::election_service::Elections>,

    /// Notification registry — tag flags compiled into UDP chirp announcements.
    pub notifications: Arc<garden_common::NotificationRegistry>,
}
