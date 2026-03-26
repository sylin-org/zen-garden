//! Weaviate Vector Database Cluster Orchestrator for Zen Garden.
//!
//! Manages Weaviate multi-node clusters using Raft consensus.
//! Uses generic cluster primitives from `orchestrator-common::cluster`.

mod domain;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("Weaviate orchestrator starting");
}
