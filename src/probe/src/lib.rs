//! Garden Probe - Integration test harness for Zen Garden
//!
//! A minimal test runner that executes tests against a live garden.
//! Tests are plain Rust async functions with access to:
//! - `LiveGarden` - connected stones (discovered via UDP or HTTP)
//! - `Bag` - accumulates results across steps for holistic tracing
//!
//! ## Discovery Modes
//!
//! - **UDP Discovery** (like Rake): Broadcasts to find all stones on network
//! - **HTTP Topology**: Queries a known stone's `/api/v1/garden` endpoint
//!
//! UDP discovery caches all responding stones, enabling:
//! - Fast failover when tended stone goes offline
//! - Inter-stone communication tests (deploy to A, verify B sees chirp)

pub mod bag;
pub mod garden;
pub mod registry;
pub mod report;
pub mod tests;

pub use bag::{Bag, StepRecord, StepResult};
pub use garden::{DiscoveryInfo, DiscoveryMethod, LiveGarden, Stone};
pub use registry::{TestDef, TestFn, TestRegistry};
pub use report::TestReport;
