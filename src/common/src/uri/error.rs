//! Error types for `zen-garden:` URI parsing (URI-0003).

use thiserror::Error;

/// Categories of URI parse failure.
///
/// String categories returned by [`UriError::category`] match the
/// `error` field of `docs/specs/zen-garden-uri-test-vectors.json` so
/// the conformance corpus can drive both parsers in lockstep.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UriError {
    /// Scheme is missing or not `zen-garden`.
    #[error("invalid scheme: expected 'zen-garden:', got {found}")]
    InvalidScheme { found: String },

    /// URI has no target and no `cap=` query — the only form where an
    /// empty target is permitted.
    #[error("empty target requires a cap= query parameter")]
    EmptyTargetNoCap,

    /// Explicit-kind form `<kind>//<name>` used a kind that is not in
    /// the reserved keyword set.
    #[error("invalid kind: '{kind}' is not a recognised resource kind")]
    InvalidKind { kind: String },

    /// Bare-name cascade form used a reserved keyword as the target.
    #[error("'{name}' is a reserved keyword and cannot be used as a bare cascade target; use the explicit form '<kind>//<name>' or pick a different name")]
    ReservedNameAsTarget { name: String },

    /// Target structure is malformed — e.g. multiple `//` separators,
    /// empty kind or name, or a `//` appearing in an invalid position.
    #[error("malformed target: {detail}")]
    MalformedTarget { detail: String },

    /// Query string is structurally invalid (e.g. missing `=`, malformed
    /// `v=` value).
    #[error("malformed query: {detail}")]
    MalformedQuery { detail: String },

    /// `v=` query parameter specified an unknown scheme version.
    #[error("unsupported scheme version: {version}")]
    UnsupportedVersion { version: u32 },

    /// Percent-encoded sequence in the URI was malformed.
    #[error("malformed percent-encoding: {detail}")]
    MalformedEncoding { detail: String },
}

impl UriError {
    /// Stable string category for cross-language test corpus matching.
    pub fn category(&self) -> &'static str {
        match self {
            UriError::InvalidScheme { .. } => "invalid_scheme",
            UriError::EmptyTargetNoCap => "empty_target_no_cap",
            UriError::InvalidKind { .. } => "invalid_kind",
            UriError::ReservedNameAsTarget { .. } => "reserved_name_as_target",
            UriError::MalformedTarget { .. } => "malformed_target",
            UriError::MalformedQuery { .. } => "malformed_query",
            UriError::UnsupportedVersion { .. } => "unsupported_version",
            UriError::MalformedEncoding { .. } => "malformed_encoding",
        }
    }
}
