//! Offering abstraction boundary.
//!
//! Defines the [`Offering`] trait contract and the [`OfferingRegistry`] that
//! stores heterogeneous offering implementations. Lives outside `domain/`
//! because trait methods perform I/O.

mod registry;
mod traits;
mod types;

pub use registry::OfferingRegistry;
pub use traits::Offering;
pub use types::{
    BenchmarkSample, DiscoveryConfig, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    Sample, ServiceModel, SyncProgress,
};
