//! Topology domain errors.
//!
//! Per ARCH-0020, Topology is a persistent aggregate (second after
//! Offerings). Store and transport operations return `TopologyError`;
//! pure cache mutations are infallible.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TopologyError {
    #[error("topology store operation failed: {0}")]
    Store(#[from] anyhow::Error),

    #[error("chirp transport failed: {0}")]
    Chirp(String),
}
