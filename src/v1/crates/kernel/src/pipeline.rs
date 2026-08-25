//! Typed startup (CODE-RULES R0.4, L17).
//!
//! A step is a name plus a fallible producer. Steps run in order, log
//! STARTED → READY, and abort loudly with their name on failure. Capabilities
//! accumulate: each step's outputs are later steps' inputs, and nothing
//! tears down or skips. The garden is never half-built.

use std::future::Future;

/// Run one named startup step. The name is the unit of failure reporting.
///
/// The abort is a deliberate, single-site panic (R0.4/L17): startup must be
/// loud and total — the garden is never half-built. This is kernel wiring,
/// not domain logic; no external data reaches this path.
#[allow(clippy::panic)]
pub async fn step<T, E, F>(name: &str, f: F) -> T
where
    F: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    tracing::info!(step = name, "STARTED");
    match f.await {
        Ok(value) => {
            tracing::info!(step = name, "READY");
            value
        }
        Err(e) => {
            tracing::error!(step = name, error = %e, "FAILED — aborting startup");
            panic!("startup step '{name}' failed: {e}");
        }
    }
}

/// A step that spawns background work; readiness is the spawn itself.
pub fn spawn_step(name: &'static str, task: tokio::task::JoinHandle<()>) {
    tracing::info!(step = name, "STARTED (background)");
    tokio::spawn(async move {
        match task.await {
            Ok(()) => tracing::info!(step = name, "finished"),
            Err(e) => tracing::error!(step = name, error = %e, "background task failed"),
        }
    });
}
