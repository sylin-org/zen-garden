//! Layered connection architecture (RAKE-0011)
//!
//! Three layers, each composing the one below:
//!
//! - **resolution** (Layer 3): pure endpoint resolution with provenance
//! - **stone** (Layer 2): bound connection — endpoint + HTTP client + typed API
//! - **resilient** (Layer 1): automatic recovery on TCP failure

pub mod resilient;
pub mod resolution;
pub mod stone;
