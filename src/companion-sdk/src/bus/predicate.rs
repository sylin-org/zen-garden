//! [`Predicate`] — a small tree expression for matching parsed
//! descriptors.
//!
//! Every predicate on match returns a *specificity score* (a `u32`)
//! that the claim engine uses to rank multiple matching registrations.
//! More-specific predicates score higher:
//!
//! - `Eq` / `HasCapability` / `VersionCaret` — 1 point per leaf.
//! - `AllOf` — sum of the inner scores (all must match).
//! - `AnyOf` — max of the inner scores (any may match).
//!
//! Predicates deliberately cover the matching needs of the firefly
//! ADR (FIREFLY-0004) — equality on string fields, existence of a
//! capability, semver-caret on a version string. Additional operators
//! land as consumers require them.

use super::descriptor::Identification;

/// A match expression over an [`Identification`].
#[derive(Debug, Clone)]
pub enum Predicate {
    /// Top-level JSON field equals a string value. Scores 1 on match.
    Eq {
        field: &'static str,
        value: &'static str,
    },

    /// Top-level JSON field exists and is a string (any value).
    /// Scores 1 on match. Use for "we require a `display`, don't care
    /// about its shape."
    Exists { field: &'static str },

    /// `capabilities` array contains the given string. Scores 1.
    HasCapability { name: &'static str },

    /// Top-level JSON field is a semver string and satisfies the given
    /// caret constraint (`^X.Y.Z`). Scores 1.
    VersionCaret {
        field: &'static str,
        baseline: &'static str,
    },

    /// All inner predicates must match. Score is the sum of inner
    /// scores. Fails fast on first non-match.
    AllOf(Vec<Predicate>),

    /// At least one inner predicate must match. Score is the maximum
    /// of matching inner scores.
    AnyOf(Vec<Predicate>),
}

impl Predicate {
    /// Shorthand: `Eq { field, value }`.
    pub const fn eq(field: &'static str, value: &'static str) -> Self {
        Predicate::Eq { field, value }
    }

    /// Shorthand: `Exists { field }`.
    pub const fn exists(field: &'static str) -> Self {
        Predicate::Exists { field }
    }

    /// Shorthand: `HasCapability { name }`.
    pub const fn has_capability(name: &'static str) -> Self {
        Predicate::HasCapability { name }
    }

    /// Shorthand: `VersionCaret { field, baseline }`.
    pub const fn version_caret(field: &'static str, baseline: &'static str) -> Self {
        Predicate::VersionCaret { field, baseline }
    }

    /// Evaluate the predicate against `id`. Returns `Some(score)` on
    /// match, `None` otherwise.
    pub fn eval(&self, id: &Identification) -> Option<u32> {
        match self {
            Predicate::Eq { field, value } => {
                if id.string_field(field) == Some(value) {
                    Some(1)
                } else {
                    None
                }
            }
            Predicate::Exists { field } => id.string_field(field).map(|_| 1),
            Predicate::HasCapability { name } => id.has_capability(name).then_some(1),
            Predicate::VersionCaret { field, baseline } => {
                let actual = id.string_field(field)?;
                satisfies_caret(actual, baseline).then_some(1)
            }
            Predicate::AllOf(inner) => {
                let mut sum = 0u32;
                for p in inner {
                    sum = sum.checked_add(p.eval(id)?)?;
                }
                Some(sum)
            }
            Predicate::AnyOf(inner) => inner.iter().filter_map(|p| p.eval(id)).max(),
        }
    }
}

/// Caret-range semver satisfaction: `actual` is compatible with
/// `^baseline` when the leading non-zero component matches and
/// `actual >= baseline` in component order.
///
/// Deliberately small / dependency-free. For the v0.x.y range we use
/// the 0.x convention (breaking per-minor): `actual` must have the
/// same minor as `baseline` and a patch >= baseline's patch.
fn satisfies_caret(actual: &str, baseline: &str) -> bool {
    let Some(a) = parse_semver(actual) else {
        return false;
    };
    let Some(b) = parse_semver(baseline) else {
        return false;
    };

    match (a.0, b.0) {
        (0, 0) => a.1 == b.1 && a.2 >= b.2, // 0.x: same minor, >= patch
        _ if a.0 != b.0 => false,           // major must match
        _ => (a.1, a.2) >= (b.1, b.2),      // major.minor.patch semver-caret
    }
}

/// Parse a semver-ish string `MAJOR.MINOR.PATCH` into a tuple.
/// Returns `None` on malformed input.
fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id(value: serde_json::Value) -> Identification {
        Identification::from_json("firefly", value).unwrap()
    }

    #[test]
    fn eq_scores_one_on_match() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "variant": "oled",
        }));
        assert_eq!(Predicate::eq("variant", "oled").eval(&i), Some(1));
        assert_eq!(Predicate::eq("variant", "matrix").eval(&i), None);
        assert_eq!(Predicate::eq("missing", "any").eval(&i), None);
    }

    #[test]
    fn exists_passes_on_any_string() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "variant": "oled",
        }));
        assert_eq!(Predicate::exists("variant").eval(&i), Some(1));
        assert_eq!(Predicate::exists("missing").eval(&i), None);
    }

    #[test]
    fn has_capability_reads_array() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "capabilities": ["dashboard", "brightness"],
        }));
        assert_eq!(Predicate::has_capability("dashboard").eval(&i), Some(1));
        assert_eq!(Predicate::has_capability("gpu-bar").eval(&i), None);
    }

    #[test]
    fn all_of_sums_scores() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
            "variant": "oled",
            "capabilities": ["dashboard"],
        }));
        let p = Predicate::AllOf(vec![
            Predicate::eq("family", "firefly"),
            Predicate::eq("variant", "oled"),
            Predicate::has_capability("dashboard"),
        ]);
        assert_eq!(p.eval(&i), Some(3));
    }

    #[test]
    fn all_of_fails_on_any_mismatch() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "variant": "oled",
        }));
        let p = Predicate::AllOf(vec![
            Predicate::eq("variant", "oled"),
            Predicate::eq("family", "not-firefly"),
        ]);
        assert_eq!(p.eval(&i), None);
    }

    #[test]
    fn any_of_takes_max_score() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
            "capabilities": ["brightness"],
        }));
        let p = Predicate::AnyOf(vec![
            Predicate::AllOf(vec![
                Predicate::eq("family", "firefly"),
                Predicate::has_capability("brightness"),
            ]),
            Predicate::eq("family", "firefly"),
        ]);
        // AllOf scores 2, single Eq scores 1, max is 2.
        assert_eq!(p.eval(&i), Some(2));
    }

    #[test]
    fn version_caret_zero_major_matches_minor() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "version": "0.2.5",
        }));
        assert_eq!(
            Predicate::version_caret("version", "0.2.0").eval(&i),
            Some(1)
        );
        assert_eq!(Predicate::version_caret("version", "0.3.0").eval(&i), None);
    }

    #[test]
    fn version_caret_rejects_older_patch() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "version": "0.2.0",
        }));
        assert_eq!(Predicate::version_caret("version", "0.2.5").eval(&i), None);
    }

    #[test]
    fn version_caret_one_major_allows_minor_bump() {
        let i = id(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "version": "1.5.0",
        }));
        assert_eq!(
            Predicate::version_caret("version", "1.2.0").eval(&i),
            Some(1)
        );
        assert_eq!(Predicate::version_caret("version", "2.0.0").eval(&i), None);
    }
}
