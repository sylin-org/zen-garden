//! Service detection - re-exported from garden_common with moss-specific extensions
//!
//! Common detection methods (command, http_probe) live in garden_common::detection
//! Moss-specific container_inspect stays here (requires Docker)

pub mod container_inspect;

// Re-export common detection methods
pub use garden_common::detection::{detect_by_command, detect_by_http_probe, DetectionResult};

// Moss-specific detection (requires Docker)
pub use container_inspect::{detect_by_container_inspect, ContainerDetector};
