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

pub mod recommendation;

// ── Future ──────────────────────────────────────────────────────
// pub mod advisor;  // Multi-offering topology advice (requires per-offering config tables)
// pub mod pins;     // Pin storage (currently handled by FeatureConfig.pins in RouterConfig)
