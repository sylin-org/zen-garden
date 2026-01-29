//! Test implementations
//!
//! Tests are plain Rust async functions that receive a LiveGarden and Bag.
//! Each test records its steps in the Bag for holistic tracing.

pub mod smoke;
pub mod discovery;
pub mod tend;
pub mod interstone;
pub mod offerings;
pub mod nourishment;
pub mod adapters;
pub mod storage;
pub mod resolution;
