//! Reserved names for the `zen-garden:` URI scheme (URI-0003).
//!
//! These names MAY NOT be used as resource names for any kind. The
//! `zen-garden:` URI grammar uses reserved keywords as kind selectors in
//! the explicit form `<kind>//<name>`; allowing a stone or bank to share
//! a name with a kind would make cascade resolution ambiguous.
//!
//! Enforcement happens at resource-creation time. Moss MUST reject
//! create requests whose proposed name matches an entry here.
//!
//! See: [URI-0003](../../../docs/decisions/URI-0003-zen-garden-urn-form-scheme.md).

/// All reserved names. Includes the seven canonical kinds, historical
/// aliases, and prophylactic reservations.
pub const RESERVED_NAMES: &[&str] = &[
    // Canonical kinds (URI-0003 §"Reserved keywords")
    "offering",
    "stone",
    "bank",
    "service",
    "companion",
    "pond",
    "garden",
    // Historical aliases
    "seed-bank",
    "tool",
    // Prophylactic reservations — components and concepts that may need
    // their own kind in future extensions
    "gateway",
    "orchestrator",
    "keystone",
    "cornerstone",
    "lantern",
    "moss",
    "pavilion",
    "rake",
];

/// Returns `true` when `name` collides with a reserved keyword.
///
/// Comparison is case-insensitive — `Offering`, `OFFERING`, and
/// `offering` are all reserved.
pub fn is_reserved(name: &str) -> bool {
    let normalised = name.trim().to_ascii_lowercase();
    RESERVED_NAMES.iter().any(|&r| r == normalised)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_kinds_are_reserved() {
        for kind in &["offering", "stone", "bank", "service", "companion", "pond", "garden"] {
            assert!(is_reserved(kind), "{kind} should be reserved");
        }
    }

    #[test]
    fn case_insensitive() {
        assert!(is_reserved("OFFERING"));
        assert!(is_reserved("Offering"));
        assert!(is_reserved("OfFeRiNg"));
    }

    #[test]
    fn whitespace_trimmed() {
        assert!(is_reserved("  stone  "));
    }

    #[test]
    fn historical_aliases_reserved() {
        assert!(is_reserved("seed-bank"));
        assert!(is_reserved("tool"));
    }

    #[test]
    fn prophylactic_reservations_held() {
        assert!(is_reserved("pavilion"));
        assert!(is_reserved("moss"));
        assert!(is_reserved("rake"));
        assert!(is_reserved("lantern"));
    }

    #[test]
    fn ordinary_names_not_reserved() {
        assert!(!is_reserved("mongodb"));
        assert!(!is_reserved("crystal-forest"));
        assert!(!is_reserved("personal"));
        assert!(!is_reserved(""));
        assert!(!is_reserved("offering-not-quite"));
    }
}
