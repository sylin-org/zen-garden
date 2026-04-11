//! Typed error for the Metrics aggregate.
//!
//! Per ARCH-0018: Metrics mutation methods are **infallible** — they
//! return `()`, not `Result<(), MetricsError>`. A failure in metrics
//! recording must never break the caller's hot path. Bugs in
//! registration/recording get a `tracing::error!` log, not a
//! propagated error.
//!
//! `MetricsError` exists as a placeholder so handlers that want a
//! uniform `Result<T, MetricsError>` return shape can use it. Current
//! variants are minimal; future extensions (threshold alert rules,
//! external-exporter ingestion failures) will add variants here.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MetricsError {
    /// Reserved for future use. Currently the Metrics aggregate
    /// returns no errors from any public method — this variant exists
    /// only so handler signatures can use a uniform `Result<T,
    /// MetricsError>` shape when they prefer it over `Option` or
    /// infallible returns.
    #[error("metrics unavailable")]
    Unavailable,
}
