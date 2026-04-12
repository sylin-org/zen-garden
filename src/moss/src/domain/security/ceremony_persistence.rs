//! Persistence trait for ceremony crash recovery.
//!
//! Relocated from `domain/traits/ceremony_persistence.rs` to the Security
//! context per ARCH-0027 (Book IX of ARCH-0017).

use crate::domain::ceremony::{Ceremony, CeremonyId};
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

/// Persistent journal for ceremony state (crash recovery).
///
/// Domain code stores and loads ceremony snapshots through this trait.
/// The infra layer provides the file-system-backed implementation.
pub trait CeremonyPersistence: Send + Sync {
    /// Persist ceremony to durable storage.
    fn persist<'a>(
        &'a self,
        ceremony: &'a Ceremony,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Load all active (non-terminal) ceremonies.
    fn load_active(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Ceremony>>> + Send + '_>>;

    /// Load a specific ceremony by ID (active or archived).
    fn load<'a>(
        &'a self,
        id: &'a CeremonyId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Ceremony>>> + Send + 'a>>;

    /// Remove a ceremony from the journal.
    fn remove<'a>(
        &'a self,
        id: &'a CeremonyId,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Prune archived ceremonies older than the given duration.
    fn prune_archive(
        &self,
        older_than: chrono::Duration,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>>;
}
