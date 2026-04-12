//! Domain-level container lifecycle event.
//!
//! Translates `bollard::models::EventMessage` into a domain type so that
//! Bollard types never leave the `docker::` module boundary.

/// A container lifecycle event from the container runtime.
///
/// Consumers receive a stream of these from `ContainerRuntime::container_events()`.
/// The domain type captures only the fields that callers actually use:
/// the container name and the action string.
#[derive(Debug, Clone)]
pub struct ContainerEvent {
    /// Container name (e.g., "zen-offering-mongodb"). Matches the
    /// Docker `name` attribute from the event actor.
    pub container_name: String,

    /// Lifecycle action: "start", "stop", "die", "kill", "destroy",
    /// or "health_status: healthy" / "health_status: unhealthy".
    pub action: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_event_captures_name_and_action() {
        let event = ContainerEvent {
            container_name: "zen-offering-mongodb".to_string(),
            action: "start".to_string(),
        };
        assert_eq!(event.container_name, "zen-offering-mongodb");
        assert_eq!(event.action, "start");
    }

    #[test]
    fn container_event_supports_health_status_action() {
        let event = ContainerEvent {
            container_name: "zen-offering-ollama--dev".to_string(),
            action: "health_status: healthy".to_string(),
        };
        assert!(event.action.starts_with("health_status:"));
        let health = event.action.trim_start_matches("health_status:").trim();
        assert_eq!(health, "healthy");
    }

    #[test]
    fn container_event_is_clone_and_debug() {
        let event = ContainerEvent {
            container_name: "zen-offering-redis".to_string(),
            action: "die".to_string(),
        };
        let cloned = event.clone();
        assert_eq!(cloned.container_name, event.container_name);
        let debug = format!("{:?}", event);
        assert!(debug.contains("redis"));
    }
}
