//! Job event emission helpers
//!
//! Provides helpers for emitting job progress events through the unified EventBus.
//! All events flow through a single SSE endpoint: `/api/v1/stone/presence/stream`
//!
//! # Event Flow
//!
//! ```text
//! emit_job_progress() ──► EventBus ──► PulseDomainBridge ──► /api/v1/stone/presence/stream
//! emit_job_started()  ──► EventBus ──► PulseDomainBridge ──► /api/v1/stone/presence/stream
//! emit_job_completed()──► EventBus ──► PulseDomainBridge ──► /api/v1/stone/presence/stream
//! emit_job_failed()   ──► EventBus ──► PulseDomainBridge ──► /api/v1/stone/presence/stream
//! ```
//!
//! # Event Types
//!
//! - `job.started` - Job began (install, remove, update)
//! - `job.progress` - Progress update (pulling image, creating container, etc.)
//! - `job.completed` - Job finished successfully
//! - `job.failed` - Job failed with error
//!
//! # Consumers
//!
//! - Portrait page activity feed
//! - Firefly LED companion
//! - Cricket audio companion
//! - CLI progress monitoring

use crate::AppState;
use crate::domain::events::JobEvent;

/// Emit a job progress event via the unified EventBus
///
/// Routes job progress through the EventBus → PulseDomainBridge → presence stream.
///
/// # Arguments
/// * `state` - Application state containing EventBus
/// * `level` - Event severity: "info", "warn", "error", "debug"
/// * `message` - Human-readable event description
/// * `job_id` - Job UUID for tracking
/// * `offering` - The offering name this job is operating on
///
/// # Example
/// ```rust,ignore
/// emit_job_progress(&state, "info", "Pulling image...".to_string(), &job_id, "mongodb");
/// ```
pub fn emit_job_progress(
    state: &AppState,
    level: &str,
    message: String,
    job_id: &str,
    offering: &str,
) {
    let event = JobEvent::progress(job_id, offering, &message, level);
    state.event_bus.emit(event);

    // Also log to tracing for persistence
    match level {
        "error" => tracing::error!("{}", message),
        "warn" => tracing::warn!("{}", message),
        "debug" => tracing::debug!("{}", message),
        _ => tracing::info!("{}", message),
    }
}

/// Emit job started event
pub fn emit_job_started(state: &AppState, job_id: &str, offering: &str, operation: &str) {
    let event = JobEvent::started(job_id, offering, operation);
    state.event_bus.emit(event);
    tracing::info!("Job started: {} {}", operation, offering);
}

/// Emit job completed event
pub fn emit_job_completed(state: &AppState, job_id: &str, offering: &str) {
    let event = JobEvent::completed(job_id, offering);
    state.event_bus.emit(event);
    tracing::info!("Job completed: {}", offering);
}

/// Emit job failed event
pub fn emit_job_failed(state: &AppState, job_id: &str, offering: &str, error: &str) {
    let event = JobEvent::failed(job_id, offering, error);
    state.event_bus.emit(event);
    tracing::error!("Job failed: {} - {}", offering, error);
}
