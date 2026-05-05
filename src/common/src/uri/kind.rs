//! Resource kinds for the `zen-garden:` URI scheme (URI-0003).
//!
//! The seven canonical kinds drive cascade resolution order and the
//! explicit-form `<kind>//<name>` selector. Cascade order is the order
//! of the variants below: offering first, garden last.

use std::fmt;

/// One of the seven canonical resource kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Offering,
    Stone,
    Bank,
    Service,
    Companion,
    Pond,
    Garden,
}

impl Kind {
    /// Cascade order: kinds in the order resolvers MUST attempt them.
    /// Note: category fallback is *not* a [`Kind`] — it is a final
    /// stage consulted when none of these match.
    pub const CASCADE_ORDER: &'static [Kind] = &[
        Kind::Offering,
        Kind::Stone,
        Kind::Bank,
        Kind::Service,
        Kind::Companion,
        Kind::Pond,
        Kind::Garden,
    ];

    /// Lowercase canonical string form (matches the URI keyword).
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Offering => "offering",
            Kind::Stone => "stone",
            Kind::Bank => "bank",
            Kind::Service => "service",
            Kind::Companion => "companion",
            Kind::Pond => "pond",
            Kind::Garden => "garden",
        }
    }

    /// Parse a kind string (case-insensitive). Returns `None` for any
    /// non-canonical kind.
    pub fn parse(s: &str) -> Option<Kind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "offering" => Some(Kind::Offering),
            "stone" => Some(Kind::Stone),
            "bank" => Some(Kind::Bank),
            "service" => Some(Kind::Service),
            "companion" => Some(Kind::Companion),
            "pond" => Some(Kind::Pond),
            "garden" => Some(Kind::Garden),
            _ => None,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_order_matches_uri_0003() {
        // URI-0003 §"Cascade order": offering → stone → bank → service →
        // companion → pond → garden
        assert_eq!(
            Kind::CASCADE_ORDER,
            &[
                Kind::Offering,
                Kind::Stone,
                Kind::Bank,
                Kind::Service,
                Kind::Companion,
                Kind::Pond,
                Kind::Garden,
            ]
        );
    }

    #[test]
    fn round_trip() {
        for kind in Kind::CASCADE_ORDER {
            let s = kind.as_str();
            assert_eq!(Kind::parse(s), Some(*kind));
        }
    }

    #[test]
    fn case_insensitive_parse() {
        assert_eq!(Kind::parse("OFFERING"), Some(Kind::Offering));
        assert_eq!(Kind::parse("Offering"), Some(Kind::Offering));
    }

    #[test]
    fn unknown_kinds_rejected() {
        assert_eq!(Kind::parse("mongodb"), None);
        assert_eq!(Kind::parse(""), None);
        assert_eq!(Kind::parse("seed-bank"), None); // historical alias, not a kind
    }
}
