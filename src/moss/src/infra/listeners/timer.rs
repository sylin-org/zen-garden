//! Timer Listener - Nurturing schedule management
//!
//! Listens for offering lifecycle events and manages nurturing timers:
//! - Deployed: Creates a new nurturing timer for the offering
//! - Removed/Destroyed: Removes the nurturing timer
//! - Renamed: Updates the timer name
//!
//! Features per (action_type, offering_id) debouncing to avoid spamming the OS scheduler
//! during rapid events. Different action types for the same offering pass immediately.
//!
//! This is a stub implementation - actual timer creation depends on the platform:
//! - Linux: systemd timers (zen-nurturing-{name}.timer)
//! - Windows: Task Scheduler tasks

use crate::domain::events::OfferingEvent;
use crate::infra::event_bus::EventListener;
use garden_common::infra::StringPairDebouncer;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Default debounce duration for timer actions (1 second)
pub const DEFAULT_TIMER_DEBOUNCE_MS: u64 = 1000;

/// Timer action to be performed
#[derive(Debug, Clone)]
pub enum TimerAction {
    /// Create a new timer for the offering
    Create {
        offering_id: String,
        name: String,
    },
    /// Remove the timer for the offering
    Remove {
        offering_id: String,
        name: String,
    },
    /// Rename the timer
    Rename {
        offering_id: String,
        old_name: String,
        new_name: String,
    },
}

impl TimerAction {
    /// Get the offering_id from any action variant
    pub fn offering_id(&self) -> &str {
        match self {
            Self::Create { offering_id, .. } => offering_id,
            Self::Remove { offering_id, .. } => offering_id,
            Self::Rename { offering_id, .. } => offering_id,
        }
    }

    /// Get the action type as a string for debounce keying
    pub fn action_type(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::Remove { .. } => "remove",
            Self::Rename { .. } => "rename",
        }
    }
}

/// Callback for timer management actions
pub type TimerCallback = Arc<dyn Fn(TimerAction) + Send + Sync>;

/// Listener that manages nurturing timers based on lifecycle events
///
/// Debounces per (action_type, offering_id) to prevent spamming the OS scheduler.
/// Different action types for the same offering pass immediately.
pub struct TimerListener {
    /// Callback to perform timer actions
    action_fn: Option<TimerCallback>,
    /// Debouncer using shared garden_common implementation
    debouncer: StringPairDebouncer,
    /// Pending actions (for testing/inspection)
    pending_actions: RwLock<Vec<TimerAction>>,
}

impl TimerListener {
    /// Create a new timer listener without a callback
    ///
    /// Actions will be recorded but not executed.
    /// Use this for testing or when timer management is not yet implemented.
    pub fn new() -> Self {
        Self::with_debounce(None, Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS))
    }

    /// Create a new timer listener with the given callback
    pub fn with_callback(action_fn: TimerCallback) -> Self {
        Self::with_debounce(Some(action_fn), Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS))
    }

    /// Create with custom debounce duration
    pub fn with_debounce(action_fn: Option<TimerCallback>, debounce: Duration) -> Self {
        Self {
            action_fn,
            debouncer: StringPairDebouncer::new(debounce),
            pending_actions: RwLock::new(Vec::new()),
        }
    }

    /// Get pending actions (for testing)
    pub async fn pending_actions(&self) -> Vec<TimerAction> {
        self.pending_actions.read().await.clone()
    }

    /// Clear pending actions
    pub async fn clear_pending(&self) {
        self.pending_actions.write().await.clear();
    }
}

impl Default for TimerListener {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EventListener for TimerListener {
    async fn on_event(&self, event: &OfferingEvent) {
        // Only process timer-relevant events
        if !event.should_manage_timers() {
            return;
        }

        let action = match event {
            OfferingEvent::Deployed { offering_id, name, .. } => {
                TimerAction::Create {
                    offering_id: offering_id.clone(),
                    name: name.clone(),
                }
            }
            OfferingEvent::Removed { offering_id, name, .. } |
            OfferingEvent::Destroyed { offering_id, name, .. } => {
                TimerAction::Remove {
                    offering_id: offering_id.clone(),
                    name: name.clone(),
                }
            }
            OfferingEvent::Renamed { offering_id, old_name, new_name, .. } => {
                TimerAction::Rename {
                    offering_id: offering_id.clone(),
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                }
            }
            _ => return,
        };

        // Check debounce per (action_type, offering_id)
        if !self.debouncer.should_pass_str(action.action_type(), action.offering_id()) {
            tracing::trace!(
                action_type = action.action_type(),
                offering_id = action.offering_id(),
                "Debouncing timer action (same action+offering within window)"
            );
            return;
        }

        tracing::debug!(
            action = ?action,
            "Timer action triggered"
        );

        // Record the action
        self.pending_actions.write().await.push(action.clone());

        // Execute callback if provided
        if let Some(ref callback) = self.action_fn {
            callback(action);
        }
    }

    fn name(&self) -> &'static str {
        "timer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[tokio::test]
    async fn test_timer_create_on_deploy() {
        let listener = TimerListener::new();

        let event = OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7");
        listener.on_event(&event).await;

        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            TimerAction::Create { offering_id, name } => {
                assert_eq!(offering_id, "id-1");
                assert_eq!(name, "mongodb");
            }
            _ => panic!("Expected Create action"),
        }
    }

    #[tokio::test]
    async fn test_timer_remove_on_destroy() {
        let listener = TimerListener::new();

        let event = OfferingEvent::destroyed("id-1", "mongodb", "stone-01");
        listener.on_event(&event).await;

        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            TimerAction::Remove { offering_id, name } => {
                assert_eq!(offering_id, "id-1");
                assert_eq!(name, "mongodb");
            }
            _ => panic!("Expected Remove action"),
        }
    }

    #[tokio::test]
    async fn test_timer_rename() {
        let listener = TimerListener::new();

        let event = OfferingEvent::renamed("id-1", "mongodb", "my-mongo", "stone-01");
        listener.on_event(&event).await;

        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            TimerAction::Rename { offering_id, old_name, new_name } => {
                assert_eq!(offering_id, "id-1");
                assert_eq!(old_name, "mongodb");
                assert_eq!(new_name, "my-mongo");
            }
            _ => panic!("Expected Rename action"),
        }
    }

    #[tokio::test]
    async fn test_no_timer_on_start_stop() {
        let listener = TimerListener::new();

        listener.on_event(&OfferingEvent::started("id-1", "mongodb", "stone-01")).await;
        listener.on_event(&OfferingEvent::stopped("id-1", "mongodb", "stone-01")).await;

        let actions = listener.pending_actions().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_debounce_same_action_type() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(None, Duration::from_millis(100));

        // Rapid create actions should debounce
        listener.on_event(&OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7")).await;
        listener.on_event(&OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:8")).await;

        // Only first action should execute
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);
    }

    #[tokio::test]
    async fn test_different_action_types_not_debounced() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(None, Duration::from_millis(100));

        // Deploy then remove = different action types, both pass
        listener.on_event(&OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7")).await;
        listener.on_event(&OfferingEvent::destroyed("id-1", "mongodb", "stone-01")).await;

        // Both actions should execute (different action types)
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 2);
    }

    #[tokio::test]
    async fn test_debounce_window_expires() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(None, Duration::from_millis(50));

        // First deploy
        listener.on_event(&OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7")).await;
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        // Wait for debounce window
        sleep(Duration::from_millis(100));

        // Now another deploy should execute
        listener.on_event(&OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:8")).await;
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 2);
    }
}
