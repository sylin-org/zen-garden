//! Health bounded context — per-offering health probing and stone-level
//! system health.
//!
//! ## Architecture (ARCH-0024)
//!
//! The Health aggregate is a stateless command facade that orchestrates the
//! probe→transition→mutation→event pipeline for per-offering health. It
//! delegates probe execution through the [`HealthProbe`] port and offering
//! mutation through the Offerings aggregate's typed API.
//!
//! Stone-level system health (disk, memory, docker, initialization) lives
//! in the [`system`] submodule and serves the `/api/health` endpoint.

pub mod aggregate;
pub mod event;
pub mod probe;
pub mod system;
pub mod wait;

#[cfg(test)]
mod tests;

pub use aggregate::Health;
pub use event::{HealthChangeKind, HealthChanged};
pub use probe::{DockerHealthProbe, HealthProbe, HealthProbeResult};
pub use system::{
    build_disk_component, build_memory_component, check_disk_health, check_memory_health,
    determine_overall_status,
};
pub use wait::{HEALTH_POLL_INTERVAL, poll_until_healthy};
