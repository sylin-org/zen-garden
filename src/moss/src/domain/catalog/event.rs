//! `CatalogChanged` — internal domain event stream for the `Catalog`
//! aggregate.
//!
//! Two kinds: `Loaded` (first successful population from cache or
//! fresh rebuild) and `Rebuilt` (force-rebuild completed). The catalog
//! is mostly inert — it loads once, rebuilds at most twice per process
//! start, and is read-only thereafter.

use serde::Serialize;

use super::index::OfferingsFingerprint;

/// How the catalog was populated on the `Loaded` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadSource {
    /// Populated from on-disk cache (fingerprint matched).
    DiskCache,
    /// Populated via a fresh rebuild (no cache or fingerprint mismatch).
    FreshRebuild,
}

/// Internal domain event emitted on every mutation of the `Catalog`
/// aggregate's state.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum CatalogChanged {
    /// First successful load (from cache or fresh rebuild).
    Loaded {
        compiled_count: usize,
        fingerprint: OfferingsFingerprint,
        source: LoadSource,
    },
    /// Force-rebuild completed. Fingerprint may match (no-op transition)
    /// or differ (actual swap).
    Rebuilt {
        compiled_count: usize,
        fingerprint: OfferingsFingerprint,
        fingerprint_changed: bool,
    },
}

/// Metric kind for `Metrics::record_domain_event` — one variant per
/// `CatalogChanged` shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Loaded,
    Rebuilt,
}

impl ChangeKind {
    /// Static list of all kind names — passed to
    /// `Metrics::register_domain` so the per-kind counters are
    /// pre-populated and the hot-path reads are lock-free.
    pub const ALL_NAMES: &'static [&'static str] = &["loaded", "rebuilt"];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Rebuilt => "rebuilt",
        }
    }
}

impl CatalogChanged {
    pub fn kind(&self) -> ChangeKind {
        match self {
            Self::Loaded { .. } => ChangeKind::Loaded,
            Self::Rebuilt { .. } => ChangeKind::Rebuilt,
        }
    }
}
