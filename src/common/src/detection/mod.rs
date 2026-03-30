pub mod command;
pub mod http_probe;
pub mod inventory;
pub mod matcher;
pub mod pipeline;

// Re-export commonly used types
pub use command::{detect_by_command, DetectionResult};
pub use http_probe::detect_by_http_probe;
pub use inventory::{ProcessInfo, SystemSnapshot};
pub use matcher::{match_processes, ProcessMatch, ProcessSignature};
pub use pipeline::{DetectionPipeline, HealthCheck, PipelineResult, PortConfig};
