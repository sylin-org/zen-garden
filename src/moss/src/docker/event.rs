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
