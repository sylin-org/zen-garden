//! Reclaimers — adapters that expose each disk consumer's own cleanup
//! mechanism through the [`Reclaimable`](super::Reclaimable) port.
//!
//! Each reclaimer holds only what it needs to do its job and keeps its
//! deletion logic to itself; the governor orchestrates, it does not reach in.

pub mod image;
pub mod snapshot;

pub use image::HarvestImageReclaimer;
pub use snapshot::SnapshotReclaimer;
