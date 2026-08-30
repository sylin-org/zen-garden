//! Offerings — the garden's reason to exist (docs/v1/OFFERINGS.md).
//!
//! The registry is this stone's hot map of placed work (active + candidate
//! pools); the runtime seam is the pluggable substrate beneath *managed*
//! work only. Modes know the registry; runtimes know containers. The
//! detection domain (detect) watches the host's containers and adopts
//! what the catalog recognizes — it observes, it never operates.

pub mod capabilities;
pub mod will;
pub mod compile;
pub mod converge;
pub mod detect;
pub mod directory;
pub mod docker;
pub mod evaluate;
pub mod events;
pub mod facts;
pub mod manifest;
pub mod model;
pub mod ports;
pub mod provenance;
pub mod record;
pub mod registry;
pub mod rehearse;
pub mod runtime;
pub mod service;
pub mod storage;
