pub mod command;
pub mod http_probe;
pub mod inventory;
pub mod matcher;
pub mod pipeline;

// Re-export commonly used types
pub use command::{DetectionResult, detect_by_command};
pub use http_probe::detect_by_http_probe;
pub use inventory::{ProcessInfo, SystemSnapshot};
pub use matcher::{ProcessMatch, ProcessSignature, match_processes};
pub use pipeline::{DetectionPipeline, HealthCheck, PipelineResult, PortConfig};
