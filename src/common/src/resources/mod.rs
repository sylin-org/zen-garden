//! Resources module — system resource snapshot collection and hardware detection.
//!
//! Contains reusable resource collection:
//! - System resources (CPU, memory, disk, network, OS info)
//! - Hardware detection (GPU, storage, AI runtime)
//!
//! Note: in moss's ubiquitous language, "resources" refers to hardware
//! state snapshots (dynamic). "Metrics" refers to software observability
//! (counters, latencies, event flow). See `domain::metrics` in garden-moss
//! for the observability aggregate.

pub mod system;
