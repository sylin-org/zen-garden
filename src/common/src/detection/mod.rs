pub mod command;
pub mod http_probe;

// Re-export commonly used types
pub use command::{detect_by_command, DetectionResult};
pub use http_probe::detect_by_http_probe;
