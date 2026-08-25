//! Timeout Constants
//! Centralized timeout values with GARDEN_ environment variable overrides

use std::time::Duration;

/// Parse environment variable as duration in seconds, returning default if not set or invalid
fn env_duration_secs(var_name: &str, default_secs: u64) -> Duration {
    std::env::var(var_name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default_secs))
}

/// Parse environment variable as duration in milliseconds, returning default if not set or invalid
fn env_duration_millis(var_name: &str, default_millis: u64) -> Duration {
    std::env::var(var_name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(default_millis))
}

// ============================================================================
// Discovery Timeouts
// ============================================================================

/// Discovery broadcast timeout (default 3s)
pub fn discovery_timeout() -> Duration {
    env_duration_secs("GARDEN_DISCOVERY_TIMEOUT_SECS", 3)
}

/// Quick discovery timeout (default 2s)
pub fn discovery_quick_timeout() -> Duration {
    env_duration_secs("GARDEN_DISCOVERY_QUICK_TIMEOUT_SECS", 2)
}

// ============================================================================
// Cache TTL
// ============================================================================

/// Cache time-to-live (default 90s)
pub fn cache_ttl() -> Duration {
    env_duration_secs("GARDEN_CACHE_TTL_SECS", 90)
}

// ============================================================================
// HTTP Timeouts
// ============================================================================

/// HTTP request timeout (default 30s)
pub fn http_request_timeout() -> Duration {
    env_duration_secs("GARDEN_HTTP_REQUEST_TIMEOUT_SECS", 30)
}

/// HTTP connection timeout (default 5s)
pub fn http_connect_timeout() -> Duration {
    env_duration_secs("GARDEN_HTTP_CONNECT_TIMEOUT_SECS", 5)
}

// ============================================================================
// Retry and First-Boot Timeouts
// ============================================================================

/// First-boot retry delay (default 3s)
pub fn first_boot_retry_delay() -> Duration {
    env_duration_secs("GARDEN_FIRST_BOOT_RETRY_DELAY_SECS", 3)
}

/// First-boot total window (default 60s)
pub fn first_boot_window() -> Duration {
    env_duration_secs("GARDEN_FIRST_BOOT_WINDOW_SECS", 60)
}

/// First-boot maximum attempts (default 20)
pub fn first_boot_max_attempts() -> u32 {
    std::env::var("GARDEN_FIRST_BOOT_MAX_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20)
}

// ============================================================================
// Polling Intervals
// ============================================================================

/// Standard polling interval (default 1s)
pub fn poll_interval_1s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_1S", 1)
}

/// 2-second polling interval
pub fn poll_interval_2s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_2S", 2)
}

/// 5-second polling interval
pub fn poll_interval_5s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_5S", 5)
}

/// 10-second polling interval
pub fn poll_interval_10s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_10S", 10)
}

/// 15-second polling interval
pub fn poll_interval_15s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_15S", 15)
}

/// 30-second polling interval
pub fn poll_interval_30s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_30S", 30)
}

/// Fast resources collection interval (CPU, memory, uptime) - default 5s.
///
/// Environment variable name kept as `GARDEN_METRICS_FAST_INTERVAL_SECS`
/// for backwards compatibility with existing deployment configurations.
pub fn resources_fast_interval() -> Duration {
    env_duration_secs("GARDEN_METRICS_FAST_INTERVAL_SECS", 5)
}

/// Disk resources collection interval (slower filesystem stats) - default 30s.
///
/// Environment variable name kept as `GARDEN_METRICS_DISK_INTERVAL_SECS`
/// for backwards compatibility with existing deployment configurations.
pub fn resources_disk_interval() -> Duration {
    env_duration_secs("GARDEN_METRICS_DISK_INTERVAL_SECS", 30)
}

/// 45-second polling interval
pub fn poll_interval_45s() -> Duration {
    env_duration_secs("GARDEN_POLL_INTERVAL_45S", 45)
}

/// Short sleep duration (default 100ms)
pub fn sleep_short() -> Duration {
    env_duration_millis("GARDEN_SLEEP_SHORT_MS", 100)
}

/// Medium sleep duration (default 500ms)
pub fn sleep_medium() -> Duration {
    env_duration_millis("GARDEN_SLEEP_MEDIUM_MS", 500)
}

// ============================================================================
// Subprocess Timeouts (storage device operations)
// ============================================================================

/// Timeout for mount/umount subprocess commands (default 30s)
///
/// Mount operations on dead or unresponsive devices can hang indefinitely.
/// This timeout ensures the system recovers rather than blocking a task forever.
pub fn subprocess_mount_timeout() -> Duration {
    env_duration_secs("GARDEN_SUBPROCESS_MOUNT_TIMEOUT_SECS", 30)
}

/// Timeout for fast device-query commands: blkid, lsblk, df, blockdev (default 10s)
///
/// These are normally sub-second but can stall on dying storage controllers.
pub fn subprocess_query_timeout() -> Duration {
    env_duration_secs("GARDEN_SUBPROCESS_QUERY_TIMEOUT_SECS", 10)
}

/// Maximum consecutive mount-recovery failures before exponential backoff (default 5)
pub fn mount_recovery_backoff_threshold() -> u32 {
    std::env::var("GARDEN_MOUNT_RECOVERY_BACKOFF_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5)
}

/// Maximum consecutive mount-recovery failures before giving up (default 50)
pub fn mount_recovery_max_attempts() -> u32 {
    std::env::var("GARDEN_MOUNT_RECOVERY_MAX_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
}

// ============================================================================
// Docker Timeouts
// ============================================================================

// ============================================================================
// Operational Timeouts (API handlers, companion forwarding, pond)
// ============================================================================

/// Companion command forwarding timeout (default 5s)
pub fn companion_command_timeout() -> Duration {
    env_duration_secs("GARDEN_COMPANION_COMMAND_TIMEOUT_SECS", 5)
}

/// Nourishment execution timeout per stone (default 10s)
pub fn nourishment_timeout() -> Duration {
    env_duration_secs("GARDEN_NOURISHMENT_TIMEOUT_SECS", 10)
}

/// Capability check / offering capability probe timeout (default 20s)
pub fn capability_check_timeout() -> Duration {
    env_duration_secs("GARDEN_CAPABILITY_CHECK_TIMEOUT_SECS", 20)
}

/// Pond join / cornerstone lookup timeout (default 15s)
pub fn pond_join_timeout() -> Duration {
    env_duration_secs("GARDEN_POND_JOIN_TIMEOUT_SECS", 15)
}

/// Pond short operation timeout (default 5s)
pub fn pond_operation_timeout() -> Duration {
    env_duration_secs("GARDEN_POND_OPERATION_TIMEOUT_SECS", 5)
}

/// TLS retry sleep between attempts (default 8s)
pub fn tls_retry_delay() -> Duration {
    env_duration_secs("GARDEN_TLS_RETRY_DELAY_SECS", 8)
}

/// Companion process startup wait (default 200ms)
pub fn companion_startup_wait() -> Duration {
    env_duration_millis("GARDEN_COMPANION_STARTUP_WAIT_MS", 200)
}

// ============================================================================
// Docker Timeouts
// ============================================================================

/// Garden-wide hardware inspection per-peer timeout (default 10s)
pub fn garden_inspect_timeout() -> Duration {
    env_duration_secs("GARDEN_INSPECT_TIMEOUT_SECS", 10)
}

/// Stall detection timeout for Docker image pulls (default 5 minutes).
///
/// This is a **TTL-with-no-activity** timeout — not a wall-clock cap.
/// The timer resets each time Docker sends a progress event (layer download
/// progress, extraction status, etc.). It only fires when Docker goes
/// completely silent for this duration, indicating a genuine stall (network
/// failure, registry hang, DNS timeout, etc.).
///
/// Legitimate large-image pulls can take longer than 5 minutes total, and
/// that's fine — as long as Docker keeps sending progress events, the
/// timer keeps resetting.
pub fn docker_pull_stall_timeout() -> Duration {
    env_duration_secs("GARDEN_DOCKER_PULL_STALL_TIMEOUT_SECS", 300)
}
