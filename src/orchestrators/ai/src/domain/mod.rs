//! Pure domain layer — zero I/O, zero async.
//!
//! All algorithms operate on plain data structures. No `tokio`, `reqwest`,
//! or `std::fs` imports permitted. Only `std`, `serde`, `chrono`.

pub mod types;

// ── Generalized from Ollama orchestrator ────────────────────────
pub mod demand;
pub mod fitness;
pub mod gpu_catalog;
pub mod lease;
pub mod metrics;
pub mod placement;
pub mod policy;
pub mod reconciliation;
pub mod routing;
pub mod tiering;

// ── Pending (Phase 2+) ─────────────────────────────────────────
// pub mod advisor;
// pub mod pins;
// pub mod recommendation;
