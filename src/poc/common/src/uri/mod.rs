//! Parser and types for the `zen-garden:` URI scheme (URI-0003).
//!
//! The URI scheme expresses *intent*: "give me X." The resolver decides
//! where X comes from. The grammar is URN-form (canonical
//! `zen-garden:`), with URL-form (`zen-garden://`) accepted as a
//! tolerant alias that normalises to URN-form on round-trip.
//!
//! See [URI-0003](../../../docs/decisions/URI-0003-zen-garden-urn-form-scheme.md)
//! for the full grammar specification.
//!
//! # Examples
//!
//! ```
//! use garden_common::uri::ZenGardenUri;
//!
//! // Bare-name cascade.
//! let uri = ZenGardenUri::parse("zen-garden:mongodb").unwrap();
//! assert_eq!(uri.target_name.as_deref(), Some("mongodb"));
//! assert!(uri.kind.is_none());
//!
//! // URL-form alias parses to the same intent.
//! let urn = ZenGardenUri::parse("zen-garden:mongodb").unwrap();
//! let url = ZenGardenUri::parse("zen-garden://mongodb").unwrap();
//! assert_eq!(urn.canonical(), url.canonical());
//!
//! // Capability-only query (empty target).
//! let uri = ZenGardenUri::parse("zen-garden:?cap=s3").unwrap();
//! assert!(uri.target_name.is_none());
//! assert_eq!(uri.capabilities, vec!["s3"]);
//! ```

mod canonical;
mod error;
mod kind;
mod parser;

pub use error::UriError;
pub use kind::Kind;

use std::fmt;

/// A parsed `zen-garden:` URI.
///
/// Constructed by [`ZenGardenUri::parse`]. The struct fields are public
/// for ergonomic destructuring; mutating them outside this module risks
/// breaking round-trip canonical equality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZenGardenUri {
    /// `Some` when the URI used the explicit `<kind>//<name>` form.
    /// `None` when bare-name cascade is used or when the target is empty.
    pub kind: Option<Kind>,

    /// The target name, lowercased. `None` only when the URI is a
    /// capability-only query (empty target with `?cap=...`).
    pub target_name: Option<String>,

    /// Optional instance qualifier from `<name>:<instance>`. Lowercased.
    pub target_instance: Option<String>,

    /// Sub-path after the target. Trailing slashes stripped.
    pub sub_path: Option<String>,

    /// Capabilities from `?cap=` queries. Sorted, deduplicated, lowercased.
    pub capabilities: Vec<String>,

    /// Action verb from `?action=` (e.g. `wish`, `logs`, `restart`).
    pub action: Option<String>,

    /// Replica/stone pin from `?at=`.
    pub at: Option<String>,

    /// Tags from `?tags=` queries. Sorted, deduplicated, lowercased.
    pub tags: Vec<String>,

    /// Wire-protocol hint from `?protocol=`. Resolver hint, not a hard
    /// constraint.
    pub protocol_hint: Option<String>,

    /// Fragment after `#`. Percent-decoded.
    pub fragment: Option<String>,

    /// Scheme version. Default 1. Currently the only supported value.
    pub version: u32,
}

impl ZenGardenUri {
    /// Parse a string into a [`ZenGardenUri`].
    ///
    /// Both URN-form (`zen-garden:`) and URL-form (`zen-garden://`)
    /// are accepted; both produce the same canonical output.
    pub fn parse(input: &str) -> Result<Self, UriError> {
        parser::parse(input)
    }

    /// Returns `true` when the URI used the explicit `<kind>//<name>` form.
    pub fn kind_explicit(&self) -> bool {
        self.kind.is_some()
    }

    /// Returns `true` when the URI has no target — i.e. it is a
    /// capability-only query.
    pub fn is_capability_query(&self) -> bool {
        self.target_name.is_none()
    }

    /// Render the canonical URN-form string. Two URIs that parse to
    /// equivalent intents produce equal canonical strings.
    pub fn canonical(&self) -> String {
        canonical::render(self)
    }
}

impl fmt::Display for ZenGardenUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

impl std::str::FromStr for ZenGardenUri {
    type Err = UriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}
