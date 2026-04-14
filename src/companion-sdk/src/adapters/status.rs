//! Adapter status — lifecycle telemetry the supervisor tracks per adapter.

use std::time::Instant;

/// Current state of an adapter instance, as observed by the supervisor.
///
/// Tracked for every active adapter. Book VII's `CommandTransport` will
/// expose this at `/status` for operator visibility.
#[derive(Debug, Clone)]
pub enum AdapterStatus {
    /// Adapter task has been spawned; not yet observed to have processed
    /// any events.
    Spawning,

    /// Adapter task is healthy; has received at least one event.
    Running {
        events_handled: u64,
        last_event_at: Instant,
    },

    /// Adapter task is running but recent activity indicates a problem
    /// (device error, repeated failures, ...). Optional future state —
    /// Book VI supervisor does not transition to this automatically;
    /// adapters can request the transition via a future supervisor hook.
    Degraded {
        error: String,
        since: Instant,
    },

    /// Adapter task has ended (returned from `run`). The supervisor
    /// may respawn on next discovery tick.
    Stopped,
}

impl AdapterStatus {
    /// True for `Running`.
    pub fn is_running(&self) -> bool {
        matches!(self, AdapterStatus::Running { .. })
    }

    /// True for `Stopped`.
    pub fn is_stopped(&self) -> bool {
        matches!(self, AdapterStatus::Stopped)
    }

    /// Short human-readable label for display.
    pub fn label(&self) -> &'static str {
        match self {
            AdapterStatus::Spawning => "spawning",
            AdapterStatus::Running { .. } => "running",
            AdapterStatus::Degraded { .. } => "degraded",
            AdapterStatus::Stopped => "stopped",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_running_only_for_running() {
        assert!(AdapterStatus::Running {
            events_handled: 0,
            last_event_at: Instant::now(),
        }
        .is_running());
        assert!(!AdapterStatus::Spawning.is_running());
        assert!(!AdapterStatus::Stopped.is_running());
    }

    #[test]
    fn is_stopped_only_for_stopped() {
        assert!(AdapterStatus::Stopped.is_stopped());
        assert!(!AdapterStatus::Spawning.is_stopped());
    }

    #[test]
    fn label_matches_variant() {
        assert_eq!(AdapterStatus::Spawning.label(), "spawning");
        assert_eq!(AdapterStatus::Stopped.label(), "stopped");
        assert_eq!(
            AdapterStatus::Running {
                events_handled: 1,
                last_event_at: Instant::now()
            }
            .label(),
            "running"
        );
    }
}
