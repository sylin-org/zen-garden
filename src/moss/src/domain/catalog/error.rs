//! `CatalogError` — typed error enum for the `Catalog` aggregate.
//!
//! This is the **first domain aggregate with typed errors** in the
//! ARCH-0017 epic. Prior aggregates (Metrics, Tool, Topology, Jobs)
//! are either infallible or propagate `anyhow::Result` at adapter
//! boundaries. Catalog introduces structured error variants that
//! carry domain context (which offering failed compilation, which
//! I/O op failed) so callers can pattern-match on failure mode
//! without parsing error messages.
//!
//! The deviation is recorded in `docs/specs/domain-aggregates.md`
//! (Ch6 of ARCH-0022).

/// Typed error for `Catalog` aggregate commands.
///
/// Commands (`load`, `rebuild`) return `Result<(), CatalogError>`.
/// Callers at the API boundary wrap this in `anyhow::Error` for the
/// existing 5xx path; domain-internal callers can pattern-match.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// Failed to compute the manifest hash for fingerprint comparison.
    #[error("failed to hash manifests for fingerprint")]
    ManifestHashFailed(#[source] anyhow::Error),

    /// A single offering's template could not be compiled.
    #[error("failed to compile offering {offering}")]
    CompilationFailed {
        offering: String,
        #[source]
        source: anyhow::Error,
    },

    /// Failed to read the cached catalog index from disk.
    #[error("failed to read catalog cache from disk")]
    CacheReadFailed(#[source] anyhow::Error),

    /// Failed to persist the catalog index to disk.
    #[error("failed to write catalog cache to disk")]
    CacheWriteFailed(#[source] anyhow::Error),
}
