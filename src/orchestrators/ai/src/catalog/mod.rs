//! Offering abstraction boundary.
//!
//! Defines the `Offering` trait and the `OfferingRegistry`. This module
//! lives between the domain layer (which never imports it) and the
//! offerings layer (which implements the trait).

pub mod registry;
pub mod traits;

pub use registry::OfferingRegistry;
pub use traits::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, Sample, ServiceModel, SyncProgress,
};
