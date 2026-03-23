//! Persistence trait for ceremony crash recovery.

use crate::domain::ceremony::{Ceremony, CeremonyId};
use anyhow::Result;
use std::future::Future;

/// Persistent journal for ceremony state (crash recovery).
///
/// Domain code stores and loads ceremony snapshots through this trait.
/// The infra layer provides the file-system-backed implementation.
pub trait CeremonyPersistence: Send + Sync {
    /// Persist ceremony to durable storage.
    ///
    /// Active ceremonies are stored separately from terminal ones
    /// so `load_active` can enumerate incomplete ceremonies on restart.
    fn persist(&self, ceremony: &Ceremony) -> impl Future<Output = Result<()>> + Send;

    /// Load all active (non-terminal) ceremonies.
    ///
    /// Used on startup to detect incomplete ceremonies from a previous run.
    fn load_active(&self) -> impl Future<Output = Result<Vec<Ceremony>>> + Send;

    /// Load a specific ceremony by ID (active or archived).
    fn load(&self, id: &CeremonyId) -> impl Future<Output = Result<Option<Ceremony>>> + Send;

    /// Remove a ceremony from the journal.
    fn remove(&self, id: &CeremonyId) -> impl Future<Output = Result<()>> + Send;

    /// Prune archived ceremonies older than the given duration.
    fn prune_archive(
        &self,
        older_than: chrono::Duration,
    ) -> impl Future<Output = Result<usize>> + Send;
}
