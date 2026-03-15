//! Persistence trait for ceremony crash recovery.

use crate::domain::ceremony::{Ceremony, CeremonyId};
use anyhow::Result;
use async_trait::async_trait;

/// Persistent journal for ceremony state (crash recovery).
///
/// Domain code stores and loads ceremony snapshots through this trait.
/// The infra layer provides the file-system–backed implementation.
#[async_trait]
pub trait CeremonyPersistence: Send + Sync {
    /// Persist ceremony to durable storage.
    ///
    /// Active ceremonies are stored separately from terminal ones
    /// so `load_active` can enumerate incomplete ceremonies on restart.
    async fn persist(&self, ceremony: &Ceremony) -> Result<()>;

    /// Load all active (non-terminal) ceremonies.
    ///
    /// Used on startup to detect incomplete ceremonies from a previous run.
    async fn load_active(&self) -> Result<Vec<Ceremony>>;

    /// Load a specific ceremony by ID (active or archived).
    async fn load(&self, id: &CeremonyId) -> Result<Option<Ceremony>>;

    /// Remove a ceremony from the journal.
    async fn remove(&self, id: &CeremonyId) -> Result<()>;

    /// Prune archived ceremonies older than the given duration.
    async fn prune_archive(&self, older_than: chrono::Duration) -> Result<usize>;
}
