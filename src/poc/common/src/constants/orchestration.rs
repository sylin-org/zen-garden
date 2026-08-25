//! Orchestration constants (ORCH-0001)
//!
//! Centralized timing, thresholds, and scoring constants for the
//! offering orchestration and autonomous resilience system.

// ============================================================================
// Election — Fitness Mode Timing
// ============================================================================

/// Quiet timeout: no new higher-scoring candidate → decision time (ms).
pub const FITNESS_QUIET_TIMEOUT_MS: u64 = 1_000;

/// Hard cap: never wait longer than this for candidates (ms).
pub const FITNESS_HARD_CAP_MS: u64 = 3_000;

// ============================================================================
// Fitness Scoring
// ============================================================================

/// Pinned fitness score — outside valid range, always wins election.
pub const FITNESS_SCORE_PINNED: i16 = 1001;

/// Minimum valid fitness score.
pub const FITNESS_SCORE_MIN: i16 = -1000;

/// Maximum valid fitness score (non-pinned).
pub const FITNESS_SCORE_MAX: i16 = 1000;

// ============================================================================
// Degradation Detection
// ============================================================================

/// Consecutive health failures before transitioning to `Degraded`.
pub const DEGRADATION_CONSECUTIVE_FAILURES: u32 = 3;

/// Interval between degradation health checks (seconds).
pub const DEGRADATION_CHECK_INTERVAL_SECS: u64 = 10;

// ============================================================================
// Sync (Replica → Primary data pull)
// ============================================================================

/// Replica poll interval (seconds).
pub const SYNC_CHECK_INTERVAL_SECS: u64 = 60;

// ============================================================================
// Resource Thresholds
// ============================================================================

/// Default memory usage % above which a stone is considered resource-stressed.
pub const DEFAULT_MEMORY_THRESHOLD_PCT: f64 = 90.0;

/// Default CPU usage % above which a stone is considered resource-stressed.
pub const DEFAULT_CPU_THRESHOLD_PCT: f64 = 95.0;

/// Default disk usage % above which a stone is considered resource-stressed.
pub const DEFAULT_DISK_THRESHOLD_PCT: f64 = 95.0;
