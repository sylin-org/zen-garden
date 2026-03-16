//! Nurturing orchestration — A/B backup scheduling and harvest archives.

use std::sync::Arc;

use crate::domain::traits::{HarvestOps, NurturingStoreOps};

/// Coordination infrastructure for the nurturing (A/B backup) pipeline.
///
/// Field path: `state.orchestration.nurturing.*`
#[derive(Clone)]
pub struct NurturingOrchestration {
    /// Harvest operations — used by ceremony phases for backup/restore.
    pub harvest_ops: Arc<dyn HarvestOps>,

    /// Nurturing store — A/B local backup slots.
    pub store: Arc<dyn NurturingStoreOps>,
}
