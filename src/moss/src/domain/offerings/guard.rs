//! Read guards for the Offerings aggregate — strangler-vine back-compat.
//!
//! Existing call sites do `state.offerings.read().await.iter()...` with the
//! field typed as `Arc<RwLock<Vec<Offering>>>`. The aggregate refactor
//! preserves that call shape by returning `ActiveGuard` from `Offerings::read()`,
//! which derefs to `&Vec<Offering>` (the active pool).
//!
//! When all 82 read sites have migrated to typed query methods
//! (`snapshot`, `find_by_id`, `with_active`), this module and the
//! `Offerings::read()` method are deleted.

use super::aggregate::OfferingsState;
use garden_common::Offering;
use std::ops::Deref;
use tokio::sync::RwLockReadGuard;

/// Back-compat read guard for the active offerings pool.
///
/// Derefs to `&Vec<Offering>` so existing code that iterates over the active
/// pool works unchanged.
pub struct ActiveGuard<'a> {
    pub(super) inner: RwLockReadGuard<'a, OfferingsState>,
}

impl Deref for ActiveGuard<'_> {
    type Target = Vec<Offering>;

    fn deref(&self) -> &Vec<Offering> {
        &self.inner.active
    }
}

/// Read guard for the adopted-candidates pool. Derefs to `&Vec<Offering>`.
pub struct CandidatesGuard<'a> {
    pub(super) inner: RwLockReadGuard<'a, OfferingsState>,
}

impl Deref for CandidatesGuard<'_> {
    type Target = Vec<Offering>;

    fn deref(&self) -> &Vec<Offering> {
        &self.inner.candidates
    }
}
