//! Compiled catalog snapshot and its fingerprint.
//!
//! Moved from `domain/offerings/catalog.rs` in Ch2 of ARCH-0022
//! (Book V of ARCH-0017). Both type names are preserved unchanged
//! because they appear in external-facing trait signatures
//! ([`crate::domain::catalog::cache::CatalogCache`]) and in disk-cache
//! serialized payloads (`{config_dir}/offerings.cache.json`), and the
//! names are already accurate: the snapshot is an index of offerings,
//! and the fingerprint is the change-detection hash over moss version
//! + hardware capabilities + manifest content.

use super::entry::CompiledOffering;

/// Fingerprint for cache invalidation.
///
/// Changes to any of the three fields trigger a full catalog rebuild
/// on the next [`crate::domain::catalog::Catalog::load`] call (Ch3+).
/// During the Ch2 strangler phase the fingerprint is checked by the
/// remaining free functions in `domain/offerings/catalog.rs` before
/// they are migrated in Ch4.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OfferingsFingerprint {
    pub moss_version: String,
    pub capabilities_hash: String,
    pub templates_hash: String,
}

/// Cached offerings index with fingerprint — the compiled catalog snapshot.
///
/// Persisted to disk at `{config_dir}/offerings.cache.json` between
/// process starts so cold starts can skip manifest re-compilation when
/// the fingerprint matches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OfferingsIndex {
    pub fingerprint: OfferingsFingerprint,
    pub generated_at: String,
    pub offerings: Vec<CompiledOffering>,
}
