//! Service detection trait for container-level inspection.

use anyhow::Result;
use async_trait::async_trait;
use garden_common::detection::DetectionResult;
use garden_common::manifests::ContainerInspectDetection;

/// Container-level service detection.
///
/// Wraps Docker container inspection for detection orchestration.
/// The common-crate detection methods (command, HTTP probe) don't
/// need this trait — they're pure I/O utilities in garden_common.
#[async_trait]
pub trait ServiceDetector: Send + Sync {
    /// Detect a service by inspecting its Docker container.
    async fn detect_by_container_inspect(
        &self,
        config: &ContainerInspectDetection,
    ) -> Result<DetectionResult>;
}
