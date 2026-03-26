//! PostgreSQL Streaming Replication Orchestrator for Zen Garden.
//!
//! Manages PostgreSQL primary/standby topology using streaming replication.
//! Uses the generic cluster primitives from `orchestrator-common::cluster`.

mod domain;
mod infra;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("PostgreSQL orchestrator starting (placeholder — cluster adapter validation)");
}
