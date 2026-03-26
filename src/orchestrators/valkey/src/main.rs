//! Valkey/Redis Sentinel Orchestrator for Zen Garden.
//!
//! Manages Valkey/Redis primary-replica topology using Sentinel for
//! automatic failover. Uses generic cluster primitives from
//! `orchestrator-common::cluster`.

mod domain;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("Valkey orchestrator starting");
}
