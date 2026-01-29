//! Garden Probe - Integration test harness for Zen Garden
//!
//! A minimal test runner that executes tests against a live garden.
//! Tests are plain Rust async functions with access to:
//! - `LiveGarden` - connected stones
//! - `Bag` - accumulates results across steps for holistic tracing

pub mod bag;
pub mod garden;
pub mod registry;
pub mod report;
pub mod tests;

pub use bag::{Bag, StepRecord, StepResult};
pub use garden::{LiveGarden, Stone};
pub use registry::{TestDef, TestFn, TestRegistry};
pub use report::TestReport;
