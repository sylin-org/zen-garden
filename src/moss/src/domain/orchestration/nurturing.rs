//! Nurturing orchestration — A/B backup scheduling and harvest archives.

use std::sync::Arc;

/// Coordination infrastructure for the nurturing (A/B backup) pipeline.
///
/// Field path: `state.orchestration.nurturing.*`
#[derive(Clone)]
pub struct NurturingOrchestration {
    /// Harvest store — backup manifests and archives.
    pub harvest: Arc<crate::infra::HarvestStore>,

    /// Nurturing store — A/B local backup slots.
    pub store:   Arc<crate::infra::NurturingStore>,
}
