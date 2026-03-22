//! Service detection trait for container-level inspection.

use anyhow::Result;
use garden_common::detection::DetectionResult;
use garden_common::manifests::ContainerInspectDetection;
use std::future::Future;

/// Container-level service detection.
///
/// Wraps Docker container inspection for detection orchestration.
/// The common-crate detection methods (command, HTTP probe) don't
/// need this trait — they're pure I/O utilities in garden_common.
pub trait ServiceDetector: Send + Sync {
    /// Detect a service by inspecting its Docker container.
    fn detect_by_container_inspect(
        &self,
        config: &ContainerInspectDetection,
    ) -> impl Future<Output = Result<DetectionResult>> + Send;
}
