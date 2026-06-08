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

/// DEPLOY-0001 mark-good delay (seconds): how long a freshly-applied build must survive before the
/// upgrade is committed (its `.old` rollback backups deleted). If moss crashes before this, the
/// `.old` binaries remain so the supervisor can roll back.
pub const MARK_GOOD_SECS: u64 = 20;

/// Process exit-code contract (DEPLOY-0001) — the shared language between moss and whatever
/// supervises it (systemd / Windows SCM / the Android watchdog). The code says WHY moss exited;
/// the supervisor decides what to do. On Linux/systemd `Restart=always` respawns on any exit and
/// `systemctl stop` still stops, so these matter mainly to the hand-rolled Android watchdog.
pub mod exit {
    /// Clean stop — the supervisor must NOT respawn (operator stop / uninstall).
    pub const STOP: i32 = 0;
    /// Crash / bind failure / stalled-shutdown force-exit — respawn WITH backoff (crash-loop guard).
    pub const FATAL: i32 = 1;
    /// A staged upgrade is pending — the supervisor must run `garden-moss pre-start` (apply) then
    /// respawn. Set when `{staging}/validated/bin` exists at exit time.
    pub const RESTART_APPLY: i32 = 10;
    /// Restart requested with no staged payload (e.g. first-boot/config reload) — respawn; the
    /// `pre-start` apply is a fast no-op.
    pub const RESTART: i32 = 11;
}
