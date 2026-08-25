//! Nurturing orchestration — A/B backup scheduling and harvest archives.

use std::sync::Arc;

/// Coordination infrastructure for the nurturing (A/B backup) pipeline.
///
/// Field path: `state.orchestration.nurturing.*`
#[derive(Clone)]
pub struct NurturingOrchestration {
    /// Harvest operations — used by ceremony phases for backup/restore.
    pub harvest_ops: Arc<crate::infra::harvest::OsHarvestOps>,

    /// Nurturing store — A/B local backup slots.
    pub store: Arc<crate::infra::NurturingStore>,
}
