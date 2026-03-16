//! Server configuration constants.
//!
//! Centralized values for HTTP server lifecycle, TCP configuration,
//! and Docker retry behavior.

/// Graceful drain deadline in seconds — time to finish in-flight requests
/// after receiving a shutdown signal.
pub const DRAIN_DEADLINE_SECS: u64 = 8;

/// Hard shutdown deadline in seconds — force-exit if the process hasn't
/// terminated by this point after the shutdown signal.
pub const HARD_DEADLINE_SECS: u64 = 15;

/// TCP listen backlog — queued connections before the OS starts rejecting.
pub const TCP_BACKLOG: i32 = 128;

/// Watchdog ping interval in seconds — sd_notify(WATCHDOG=1) cadence.
/// Must be less than `WatchdogSec` in the systemd unit file.
pub const WATCHDOG_PING_SECS: u64 = 25;

/// Docker connectivity retry attempts during startup.
pub const DOCKER_STARTUP_RETRY_ATTEMPTS: u32 = 30;

/// Delay between Docker connectivity retries during startup (seconds).
pub const DOCKER_STARTUP_RETRY_DELAY_SECS: u64 = 2;
