//! Category index for the cascade fallback stage (URI-0003 + URI-0002).
//!
//! Categories are taxonomic groupings (`database`, `document-database`,
//! `vector`, etc.) that resolve through the offering cascade as the
//! final stage. When a bare name doesn't match any of the seven kinds
//! (offering, stone, bank, service, companion, pond, garden), the
//! resolver consults this list. A category lookup returns the set of
//! offerings whose taxonomy contains the requested term.
//!
//! Categories are not reserved keywords — they are *names that the
//! cascade also tries*. Adding a new category extends this constant
//! and does not require a URI grammar change.
//!
//! See: [URI-0003](../../../docs/decisions/URI-0003-zen-garden-urn-form-scheme.md).

/// Canonical category set. Initial entries derived from existing
/// taxonomy in offering manifests.
pub const CATEGORIES: &[&str] = &[
    "database",
    "document-database",
    "relational-database",
    "key-value-store",
    "vector",
    "vector-database",
    "search-engine",
    "queue",
    "object-store",
    "storage",
    "cache",
    "stream",
    "ml-inference",
    "embedding-model",
];

/// Returns `true` when `name` matches a known category. Comparison is
/// case-insensitive.
pub fn is_category(name: &str) -> bool {
    let normalised = name.trim().to_ascii_lowercase();
    CATEGORIES.iter().any(|&c| c == normalised)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_categories_recognised() {
        assert!(is_category("database"));
        assert!(is_category("document-database"));
        assert!(is_category("vector"));
        assert!(is_category("storage"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_category("Database"));
        assert!(is_category("DATABASE"));
    }

    #[test]
    fn unknown_categories_rejected() {
        assert!(!is_category("mongodb"));
        assert!(!is_category("custom-category"));
        assert!(!is_category(""));
    }
}
