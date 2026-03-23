//! Ceremony coordination — pond init/join/unlock ceremony infrastructure.

use crate::domain::CeremonyRegistry;
use std::sync::Arc;

/// Pond ceremony coordination (`state.security.pond.ceremony`).
#[derive(Clone)]
pub struct Ceremony {
    /// Drives pond init/join/unlock ceremonies via the koi-common protocol.
    pub host:
        Arc<koi_common::ceremony::CeremonyHost<koi_certmesh::pond_ceremony::PondCeremonyRules>>,
    /// In-memory active ceremony registry.
    pub registry: Arc<CeremonyRegistry>,
    /// Persistent journal for crash recovery.
    pub journal: Arc<crate::infra::CeremonyJournal>,
}
