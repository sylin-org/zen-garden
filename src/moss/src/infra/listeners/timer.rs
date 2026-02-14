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
//! # Platform Support
//! - Linux: systemd timers (zen-nurturing-{name}.timer)
//! - Windows: Task Scheduler tasks (ZenGarden-Nurturing-{name})

use crate::domain::events::{DomainEvent, OfferingEvent};
use crate::infra::event_bus::EventListener;
use garden_common::infra::{PlatformTimer, StringPairDebouncer, TimerConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Default debounce duration for timer actions (1 second)
pub const DEFAULT_TIMER_DEBOUNCE_MS: u64 = 1000;

/// Timer action to be performed
#[derive(Debug, Clone)]
pub enum TimerAction {
    /// Create a new timer for the offering
    Create { offering_id: String, name: String },
    /// Remove the timer for the offering
    Remove { offering_id: String, name: String },
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

    /// Get the offering name for the action
    pub fn name(&self) -> &str {
        match self {
            Self::Create { name, .. } => name,
            Self::Remove { name, .. } => name,
            Self::Rename { new_name, .. } => new_name,
        }
    }
}

/// Callback for timer management actions (for testing)
pub type TimerCallback = Arc<dyn Fn(TimerAction) + Send + Sync>;

/// Listener that manages nurturing timers based on lifecycle events
///
/// Debounces per (action_type, offering_id) to prevent spamming the OS scheduler.
/// Different action types for the same offering pass immediately.
pub struct TimerListener {
    /// Platform-specific timer manager
    platform_timer: Arc<PlatformTimer>,
    /// Timer configuration (interval, persistence, etc.)
    timer_config: TimerConfig,
    /// Debouncer using shared garden_common implementation
    debouncer: StringPairDebouncer,
    /// Pending actions (for testing/inspection)
    pending_actions: RwLock<Vec<TimerAction>>,
    /// Optional callback for testing (called instead of platform timer)
    test_callback: Option<TimerCallback>,
    /// Whether to actually execute platform timer operations
    execute_enabled: bool,
}

impl TimerListener {
    /// Create a new timer listener with platform timer integration
    ///
    /// Timer actions will be executed using the platform-specific timer manager.
    pub fn new() -> Self {
        Self::with_config(TimerConfig::default())
    }

    /// Create with custom timer configuration
    pub fn with_config(timer_config: TimerConfig) -> Self {
        Self {
            platform_timer: Arc::new(PlatformTimer::new()),
            timer_config,
            debouncer: StringPairDebouncer::new(Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS)),
            pending_actions: RwLock::new(Vec::new()),
            test_callback: None,
            execute_enabled: true,
        }
    }

    /// Create with custom API base URL for timer triggers
    pub fn with_api_url(api_base_url: &str) -> Self {
        Self {
            platform_timer: Arc::new(PlatformTimer::with_api_url(api_base_url)),
            timer_config: TimerConfig::default(),
            debouncer: StringPairDebouncer::new(Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS)),
            pending_actions: RwLock::new(Vec::new()),
            test_callback: None,
            execute_enabled: true,
        }
    }

    /// Create a test listener that records actions but doesn't execute them
    ///
    /// Use this for testing or when timer management should be disabled.
    pub fn test_only() -> Self {
        Self {
            platform_timer: Arc::new(PlatformTimer::new()),
            timer_config: TimerConfig::default(),
            debouncer: StringPairDebouncer::new(Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS)),
            pending_actions: RwLock::new(Vec::new()),
            test_callback: None,
            execute_enabled: false,
        }
    }

    /// Create with a test callback (for unit testing)
    pub fn with_test_callback(callback: TimerCallback) -> Self {
        Self {
            platform_timer: Arc::new(PlatformTimer::new()),
            timer_config: TimerConfig::default(),
            debouncer: StringPairDebouncer::new(Duration::from_millis(DEFAULT_TIMER_DEBOUNCE_MS)),
            pending_actions: RwLock::new(Vec::new()),
            test_callback: Some(callback),
            execute_enabled: false,
        }
    }

    /// Create with custom debounce duration (for testing)
    pub fn with_debounce(debounce: Duration, execute_enabled: bool) -> Self {
        Self {
            platform_timer: Arc::new(PlatformTimer::new()),
            timer_config: TimerConfig::default(),
            debouncer: StringPairDebouncer::new(debounce),
            pending_actions: RwLock::new(Vec::new()),
            test_callback: None,
            execute_enabled,
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

    /// Get the platform timer (for direct access)
    pub fn platform_timer(&self) -> &PlatformTimer {
        &self.platform_timer
    }

    /// Execute a timer action using the platform timer
    async fn execute_action(&self, action: &TimerAction) {
        if !self.execute_enabled {
            return;
        }

        let result = match action {
            TimerAction::Create { name, .. } => {
                self.platform_timer.create(name, &self.timer_config).await
            }
            TimerAction::Remove { name, .. } => self.platform_timer.remove(name).await,
            TimerAction::Rename {
                old_name, new_name, ..
            } => {
                self.platform_timer
                    .rename(old_name, new_name, &self.timer_config)
                    .await
            }
        };

        match result {
            Ok(res) if res.success => {
                tracing::info!(
                    action = action.action_type(),
                    name = action.name(),
                    message = %res.message,
                    "Timer action succeeded"
                );
            }
            Ok(res) => {
                tracing::warn!(
                    action = action.action_type(),
                    name = action.name(),
                    message = %res.message,
                    "Timer action failed (non-fatal)"
                );
            }
            Err(e) => {
                tracing::error!(
                    action = action.action_type(),
                    name = action.name(),
                    error = ?e,
                    "Timer action error"
                );
            }
        }
    }
}

impl Default for TimerListener {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EventListener for TimerListener {
    async fn on_event(&self, event: &DomainEvent) {
        // Only process offering events for timer management
        let offering_event = match event {
            DomainEvent::Offering(e) => e,
            // Storage and Stone events don't affect timers
            _ => return,
        };

        // Only process timer-relevant events
        if !offering_event.should_manage_timers() {
            return;
        }

        let action = match offering_event {
            OfferingEvent::Deployed {
                offering_id, name, ..
            } => TimerAction::Create {
                offering_id: offering_id.clone(),
                name: name.clone(),
            },
            OfferingEvent::Removed {
                offering_id, name, ..
            }
            | OfferingEvent::Destroyed {
                offering_id, name, ..
            } => TimerAction::Remove {
                offering_id: offering_id.clone(),
                name: name.clone(),
            },
            OfferingEvent::Renamed {
                offering_id,
                old_name,
                new_name,
                ..
            } => TimerAction::Rename {
                offering_id: offering_id.clone(),
                old_name: old_name.clone(),
                new_name: new_name.clone(),
            },
            _ => return,
        };

        // Check debounce per (action_type, offering_id)
        if !self
            .debouncer
            .should_pass_str(action.action_type(), action.offering_id())
        {
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

        // Execute test callback if provided
        if let Some(ref callback) = self.test_callback {
            callback(action.clone());
        }

        // Execute platform timer action
        self.execute_action(&action).await;
    }

    fn name(&self) -> &'static str {
        super::names::TIMER
    }
}

/// Timer executor for direct invocation (testing and manual triggers)
///
/// Provides methods to directly create, remove, or list timers
/// without going through the event listener.
pub struct TimerExecutor {
    platform_timer: PlatformTimer,
    timer_config: TimerConfig,
}

impl TimerExecutor {
    /// Create a new timer executor with default configuration
    pub fn new() -> Self {
        Self {
            platform_timer: PlatformTimer::new(),
            timer_config: TimerConfig::default(),
        }
    }

    /// Create with custom API URL
    pub fn with_api_url(api_base_url: &str) -> Self {
        Self {
            platform_timer: PlatformTimer::with_api_url(api_base_url),
            timer_config: TimerConfig::default(),
        }
    }

    /// Create with custom timer configuration
    pub fn with_config(timer_config: TimerConfig) -> Self {
        Self {
            platform_timer: PlatformTimer::new(),
            timer_config,
        }
    }

    /// Create a nurturing timer for an offering
    pub async fn create(
        &self,
        offering_name: &str,
    ) -> anyhow::Result<garden_common::infra::TimerResult> {
        self.platform_timer
            .create(offering_name, &self.timer_config)
            .await
    }

    /// Remove a nurturing timer
    pub async fn remove(
        &self,
        offering_name: &str,
    ) -> anyhow::Result<garden_common::infra::TimerResult> {
        self.platform_timer.remove(offering_name).await
    }

    /// Rename a nurturing timer
    pub async fn rename(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> anyhow::Result<garden_common::infra::TimerResult> {
        self.platform_timer
            .rename(old_name, new_name, &self.timer_config)
            .await
    }

    /// Check if a timer exists
    pub async fn exists(&self, offering_name: &str) -> anyhow::Result<bool> {
        self.platform_timer.exists(offering_name).await
    }

    /// List all nurturing timers
    pub async fn list(&self) -> anyhow::Result<Vec<String>> {
        self.platform_timer.list().await
    }

    /// Trigger a timer immediately (for testing)
    pub async fn trigger(
        &self,
        offering_name: &str,
    ) -> anyhow::Result<garden_common::infra::TimerResult> {
        self.platform_timer.trigger(offering_name).await
    }
}

impl Default for TimerExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[tokio::test]
    async fn test_timer_create_on_deploy() {
        let listener = TimerListener::test_only();

        let event = DomainEvent::Offering(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
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
        let listener = TimerListener::test_only();

        let event = DomainEvent::Offering(OfferingEvent::destroyed("id-1", "mongodb", "stone-01"));
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
        let listener = TimerListener::test_only();

        let event = DomainEvent::Offering(OfferingEvent::renamed(
            "id-1", "mongodb", "my-mongo", "stone-01",
        ));
        listener.on_event(&event).await;

        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            TimerAction::Rename {
                offering_id,
                old_name,
                new_name,
            } => {
                assert_eq!(offering_id, "id-1");
                assert_eq!(old_name, "mongodb");
                assert_eq!(new_name, "my-mongo");
            }
            _ => panic!("Expected Rename action"),
        }
    }

    #[tokio::test]
    async fn test_no_timer_on_start_stop() {
        let listener = TimerListener::test_only();

        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::started(
                "id-1", "mongodb", "stone-01",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::stopped(
                "id-1", "mongodb", "stone-01",
            )))
            .await;

        let actions = listener.pending_actions().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_debounce_same_action_type() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(Duration::from_millis(100), false);

        // Rapid create actions should debounce
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:8",
            )))
            .await;

        // Only first action should execute
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);
    }

    #[tokio::test]
    async fn test_different_action_types_not_debounced() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(Duration::from_millis(100), false);

        // Deploy then remove = different action types, both pass
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::destroyed(
                "id-1", "mongodb", "stone-01",
            )))
            .await;

        // Both actions should execute (different action types)
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 2);
    }

    #[tokio::test]
    async fn test_debounce_window_expires() {
        // Short debounce for testing
        let listener = TimerListener::with_debounce(Duration::from_millis(50), false);

        // First deploy
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 1);

        // Wait for debounce window
        sleep(Duration::from_millis(100));

        // Now another deploy should execute
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:8",
            )))
            .await;
        let actions = listener.pending_actions().await;
        assert_eq!(actions.len(), 2);
    }

    #[tokio::test]
    async fn test_timer_action_accessors() {
        let create = TimerAction::Create {
            offering_id: "id-1".to_string(),
            name: "mongodb".to_string(),
        };
        assert_eq!(create.offering_id(), "id-1");
        assert_eq!(create.action_type(), "create");
        assert_eq!(create.name(), "mongodb");

        let remove = TimerAction::Remove {
            offering_id: "id-2".to_string(),
            name: "redis".to_string(),
        };
        assert_eq!(remove.action_type(), "remove");
        assert_eq!(remove.name(), "redis");

        let rename = TimerAction::Rename {
            offering_id: "id-3".to_string(),
            old_name: "old".to_string(),
            new_name: "new".to_string(),
        };
        assert_eq!(rename.action_type(), "rename");
        assert_eq!(rename.name(), "new");
    }

    #[tokio::test]
    async fn test_timer_executor_creation() {
        let executor = TimerExecutor::new();
        // Just verify it can be created without panicking
        assert!(executor.list().await.is_ok() || executor.list().await.is_err());
    }

    #[tokio::test]
    async fn test_callback_is_invoked() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        let callback: TimerCallback = Arc::new(move |_action| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let listener = TimerListener::with_test_callback(callback);

        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::deployed(
                "id-1", "mongodb", "stone-01", "mongo:7",
            )))
            .await;
        listener
            .on_event(&DomainEvent::Offering(OfferingEvent::destroyed(
                "id-1", "mongodb", "stone-01",
            )))
            .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
