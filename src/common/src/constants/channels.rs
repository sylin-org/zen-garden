//! Channel capacity constants.
//!
//! Named capacities for `broadcast::channel()` and `mpsc::channel()`.
//! Eliminates magic numbers scattered across channel construction sites.

/// Log stream — high-volume, bursty during startup. Consumers (SSE, file writer)
/// must tolerate lag; large buffer reduces lag frequency.
pub const LOG_STREAM: usize = 1024;

/// Tool delta events — moderate volume, one per tool state change.
pub const TOOL_DELTA: usize = 512;

/// Pulse / presence heartbeat — moderate volume, periodic.
pub const PULSE: usize = 512;

/// Storage tick / changed events — low-to-moderate volume per volume.
pub const STORAGE_EVENT: usize = 64;

/// P2P announcement / discovery events — moderate volume on busy LANs.
pub const P2P_EVENT: usize = 100;

/// SSE dashboard events (Lantern, orchestrators) — moderate volume.
pub const SSE_DASHBOARD: usize = 256;

/// Docker / network monitor reconnect events — low volume.
pub const MONITOR_EVENT: usize = 100;

/// Offerings aggregate mutation events (ARCH-0016) — low volume on
/// well-behaved stones, bursty during bulk reconciliation.
pub const OFFERINGS_EVENT: usize = 128;

/// Metrics aggregate interesting-transition events (ARCH-0018) — low
/// volume. Counter increments do NOT fire events (would flood the
/// channel under load); only task state changes, lag detection, and
/// threshold crossings fire. 128 is ample headroom for the expected
/// transition rate across all domains and tasks.
pub const METRICS_EVENT: usize = 128;
