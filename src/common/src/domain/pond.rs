//! Pond — a stone's membership state in the garden's security layer.
//!
//! A Pond is the mTLS trust domain spanning one or more stones. Its lifecycle:
//!
//! - **Solo** — the stone has not joined any pond. No mTLS.
//! - **Member** — the stone is enrolled in a pond; certificate issued
//!   by the pond's cornerstone.
//! - **Cornerstone** — the stone is the founding member of its pond
//!   and holds the certificate authority.
//!
//! Book IV surfaces Pond as a typed value for companions and adapters that
//! need to display or react to pond membership without reaching into moss's
//! internal `Security` aggregate.

use serde::{Deserialize, Serialize};

/// A stone's pond membership state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pond {
    /// Not in any pond. No mTLS, no peer enrollment.
    Solo,
    /// Member of a pond (not the cornerstone).
    Member,
    /// Cornerstone: the stone that founded the pond and holds its CA.
    Cornerstone,
}

impl Pond {
    /// Interpret a wire boolean (`pond_active`) as a [`Pond`] value. This
    /// intentionally loses the Cornerstone distinction — use the richer
    /// constructors when available.
    pub fn from_active_flag(active: bool) -> Self {
        if active { Pond::Member } else { Pond::Solo }
    }

    /// True for `Member` and `Cornerstone`. False for `Solo`.
    pub fn is_active(&self) -> bool {
        !matches!(self, Pond::Solo)
    }

    /// True only for `Cornerstone`.
    pub fn is_cornerstone(&self) -> bool {
        matches!(self, Pond::Cornerstone)
    }
}

impl Default for Pond {
    /// Default is [`Pond::Solo`] — a stone starts out in no pond.
    fn default() -> Self {
        Pond::Solo
    }
}

impl std::fmt::Display for Pond {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Pond::Solo => "solo",
            Pond::Member => "member",
            Pond::Cornerstone => "cornerstone",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_active_flag_maps_bool_to_variants() {
        assert_eq!(Pond::from_active_flag(false), Pond::Solo);
        assert_eq!(Pond::from_active_flag(true), Pond::Member);
    }

    #[test]
    fn is_active_excludes_solo() {
        assert!(!Pond::Solo.is_active());
        assert!(Pond::Member.is_active());
        assert!(Pond::Cornerstone.is_active());
    }

    #[test]
    fn is_cornerstone_only_for_cornerstone() {
        assert!(!Pond::Solo.is_cornerstone());
        assert!(!Pond::Member.is_cornerstone());
        assert!(Pond::Cornerstone.is_cornerstone());
    }

    #[test]
    fn display_uses_lowercase_names() {
        assert_eq!(format!("{}", Pond::Solo), "solo");
        assert_eq!(format!("{}", Pond::Member), "member");
        assert_eq!(format!("{}", Pond::Cornerstone), "cornerstone");
    }

    #[test]
    fn serde_round_trips() {
        for p in [Pond::Solo, Pond::Member, Pond::Cornerstone] {
            let json = serde_json::to_string(&p).unwrap();
            let back: Pond = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn serde_uses_lowercase_strings() {
        assert_eq!(serde_json::to_string(&Pond::Solo).unwrap(), "\"solo\"");
        assert_eq!(
            serde_json::to_string(&Pond::Cornerstone).unwrap(),
            "\"cornerstone\""
        );
    }
}
