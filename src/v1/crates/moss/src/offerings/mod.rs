//! Offerings — the garden's reason to exist (docs/v1/OFFERINGS.md).
//!
//! The registry is this stone's hot map of placed work (active + candidate
//! pools); the runtime seam is the pluggable substrate beneath *managed*
//! work only. Modes know the registry; runtimes know containers.

pub mod compile;
pub mod converge;
pub mod directory;
pub mod docker;
pub mod evaluate;
pub mod events;
pub mod facts;
pub mod manifest;
pub mod model;
pub mod ports;
pub mod record;
pub mod registry;
pub mod runtime;
pub mod service;
