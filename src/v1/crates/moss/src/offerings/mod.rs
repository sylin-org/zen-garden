//! Offerings — the garden's reason to exist (docs/v1/OFFERINGS.md).
//!
//! The registry is this stone's hot map of placed work (active + candidate
//! pools); the runtime seam is the pluggable substrate beneath *managed*
//! work only. Modes know the registry; runtimes know containers.

pub mod docker;
pub mod model;
pub mod registry;
pub mod service;
pub mod runtime;
