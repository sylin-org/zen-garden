// The namespace law lands with its consumers (OFFERINGS.md §1, ADR-0003).
#![allow(dead_code)]

//! Offering name grammar — the ONE definition of identity spelling.
//!
//! An offering's true name is an **FQN**: `{stem}::{instance}`. Every
//! offering is born an instance; planting `ollama` is planting
//! `ollama::default`. Neither segment may contain `:` anywhere — image
//! tags (`redis:7-alpine`) live in the single-colon space and must NEVER
//! be mistakable for a name. Operators speak the **moniker**
//! (`ollama`), which is infrastructure shorthand for the default
//! instance; machines store, hash, and announce FQNs exclusively.
//!
//! `default` is a RESERVED word: implicitly owned by every stem, never a
//! distinct second installation, and the only spelling that collapses in
//! display. Collisions between `foo::bar` and a hypothetical flat name
//! `foo_bar` cannot arise because `_`-bearing stems are legal grammar but
//! `:`-bearing ones are not — the separator space is exclusive.

use std::fmt;

/// The instance separator — two colons, always.
pub const SEPARATOR: &str = "::";
/// Reserved implicit instance; every stem owns exactly one.
pub const DEFAULT_INSTANCE: &str = "default";
/// Segment length ceiling (dirs, container names, ergonomics).
pub const MAX_SEGMENT: usize = 64;

/// Why a name refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// Zero-length segment (e.g. `a::`, `::b`, empty input).
    Empty,
    /// Characters outside the grammar `[A-Za-z0-9_-]`.
    InvalidCharacters { segment: String },
    /// Exactly one `:` seen — the classic image-tag mistake.
    LoneColon { input: String, hint: String },
    /// More than one `::` — the namespace has exactly two levels.
    TooManySeparators { input: String },
    /// Over [`MAX_SEGMENT`] characters.
    TooLong { segment: String },
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "name segment is empty"),
            Self::InvalidCharacters { segment } => write!(
                f,
                "'{segment}' may only contain letters, digits, '-' and '_'"
            ),
            Self::LoneColon { input, hint } => write!(
                f,
                "'{input}' contains a single ':' - did you mean '{hint}'? \
                 (':' is reserved for image tags; instances use '::')"
            ),
            Self::TooManySeparators { input } => {
                write!(f, "'{input}' has more than one '{SEPARATOR}'")
            }
            Self::TooLong { segment } => {
                write!(f, "'{segment}' exceeds {MAX_SEGMENT} characters")
            }
        }
    }
}

impl std::error::Error for NameError {}

/// One segment is grammar-legal.
fn well_formed(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= MAX_SEGMENT
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn check_segment(segment: &str) -> Result<(), NameError> {
    if segment.is_empty() {
        return Err(NameError::Empty);
    }
    if segment.len() > MAX_SEGMENT {
        return Err(NameError::TooLong {
            segment: segment.to_string(),
        });
    }
    if !well_formed(segment) {
        return Err(NameError::InvalidCharacters {
            segment: segment.to_string(),
        });
    }
    Ok(())
}

/// Split an EXACT FQN into `(stem, instance)`.
pub fn parse(fqn: &str) -> Result<(String, String), NameError> {
    let trimmed = fqn.trim();
    let segments: Vec<&str> = trimmed.split(SEPARATOR).collect();
    match segments.as_slice() {
        [stem] => {
            // Lone ':' hides inside one segment — catch it with a hint.
            if let Some((a, b)) = trimmed.split_once(':') {
                let b = b.trim_start_matches(':');
                return Err(NameError::LoneColon {
                    input: trimmed.to_string(),
                    hint: format!("{a}{SEPARATOR}{b}"),
                });
            }
            check_segment(stem)?;
            Ok((stem.to_string(), DEFAULT_INSTANCE.to_string()))
        }
        [stem, instance] => {
            check_segment(stem)?;
            check_segment(instance)?;
            Ok((stem.to_string(), instance.to_string()))
        }
        _ => Err(NameError::TooManySeparators {
            input: trimmed.to_string(),
        }),
    }
}

/// Join both halves back into one FQN string.
pub fn join(stem: &str, instance: &str) -> String {
    format!("{stem}{SEPARATOR}{instance}")
}

/// Canonical FQN of whatever the operator typed: trims, validates, and
/// appends the reserved default instance for bare monikers. Idempotent.
pub fn canonicalize(input: &str) -> Result<String, NameError> {
    let (stem, instance) = parse(input)?;
    Ok(join(&stem, &instance))
}

/// What humans see: the default instance's half is infrastructure noise.
/// Non-default instances render in full (`ollama::adopted` stays honest).
pub fn moniker(fqn: &str) -> String {
    let trimmed = fqn.trim();
    match trimmed.rsplit_once(SEPARATOR) {
        Some((stem, instance))
            if instance == DEFAULT_INSTANCE && !stem.is_empty() =>
        {
            stem.to_string()
        }
        _ => trimmed.to_string(),
    }
}

/// The stem half alone (provenance: which catalog manifestation).
pub fn stem_of(fqn: &str) -> String {
    parse(fqn).map(|(s, _)| s).unwrap_or_else(|_| fqn.trim().to_string())
}

/// Is this the reserved default spelling?
pub fn is_default_instance(fqn: &str) -> bool {
    parse(fqn)
        .map(|(_, i)| i == DEFAULT_INSTANCE)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn bare_monikers_canonicalize_to_default_instances() {
        assert_eq!(canonicalize("ollama").unwrap(), "ollama::default");
        assert_eq!(canonicalize("  memcached ").unwrap(), "memcached::default");
        assert_eq!(
            canonicalize("memcached").unwrap(),
            canonicalize("memcached::default").unwrap(),
            "the alias spelling collapses"
        );
    }

    #[test]
    fn explicit_instances_pass_through_unchanged() {
        assert_eq!(canonicalize("redis::prod").unwrap(), "redis::prod");
        assert_eq!(parse("ollama::adopted").unwrap(), ("ollama".into(), "adopted".into()));
    }

    #[test]
    fn single_colon_refuses_with_the_double_colon_hint() {
        // `ollama::adopted` has NO lone colons and must parse — assert the
        // happy shape first, then that a tag-shaped name refuses as LoneColon.
        assert!(canonicalize("ollama::adopted").is_ok());
        let err = canonicalize("redis:7-alpine").unwrap_err();
        assert!(
            matches!(
                err,
                NameError::LoneColon { ref hint, .. } if hint == "redis::7-alpine"
            ),
            "expected LoneColon with the corrected hint, got {err:?}"
        );
    }

    #[test]
    fn more_than_two_levels_are_rejected() {
        assert!(matches!(
            parse("a::b::c"),
            Err(NameError::TooManySeparators { .. })
        ));
    }

    #[test]
    fn grammar_bans_hostile_segments() {
        assert!(matches!(parse(""), Err(NameError::Empty)));
        assert!(matches!(parse("a::"), Err(NameError::Empty)));
        assert!(matches!(parse("::b"), Err(NameError::Empty)));
        assert!(matches!(
            parse("bad@name"),
            Err(NameError::InvalidCharacters { .. })
        ));
        let long = "x".repeat(MAX_SEGMENT + 1);
        assert!(matches!(parse(&long), Err(NameError::TooLong { .. })));
    }

    #[test]
    fn moniker_suppresses_only_the_default() {
        assert_eq!(moniker("memcached::default"), "memcached");
        assert_eq!(moniker("memcached"), "memcached");
        assert_eq!(moniker("redis::prod"), "redis::prod", "foreign instances stay honest");
        assert_eq!(moniker("ollama::adopted"), "ollama::adopted");
    }

    #[test]
    fn helpers_agree_with_parse() {
        assert_eq!(stem_of("redis::prod"), "redis");
        assert_eq!(stem_of("plain"), "plain");
        assert!(is_default_instance("plain"));
        assert!(!is_default_instance("plain::prod"));
    }
}
