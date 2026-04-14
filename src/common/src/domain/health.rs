//! Health — the five-valued vitality enum.
//!
//! Corresponds 1:1 with the `VITALITY_*` constants in
//! [`crate::constants`], preserving their string representations for serde
//! wire compatibility. A moss stone emitting `{"health": "thriving"}`
//! continues to round-trip losslessly.

use crate::constants::{
    VITALITY_DORMANT, VITALITY_NEEDS_ATTENTION, VITALITY_THRIVING, VITALITY_WILTING,
    VITALITY_WITHERING,
};
use serde::{Deserialize, Serialize};

/// A stone's (or service's) vitality.
///
/// Ordered from healthy to terminal:
/// `Thriving > NeedsAttention > Withering > Wilting`. `Dormant` is
/// orthogonal — it indicates unreachability, not health severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Health {
    /// Fully operational.
    #[serde(rename = "thriving")]
    Thriving,

    /// Warning: degraded but still functional.
    #[serde(rename = "needs attention")]
    NeedsAttention,

    /// Critical: significantly impaired.
    #[serde(rename = "withering")]
    Withering,

    /// Terminal: critically failing.
    #[serde(rename = "wilting")]
    Wilting,

    /// Offline or unreachable.
    #[serde(rename = "dormant")]
    Dormant,
}

impl Health {
    /// The canonical wire string (matches the `VITALITY_*` constants).
    pub fn as_str(&self) -> &'static str {
        match self {
            Health::Thriving => VITALITY_THRIVING,
            Health::NeedsAttention => VITALITY_NEEDS_ATTENTION,
            Health::Withering => VITALITY_WITHERING,
            Health::Wilting => VITALITY_WILTING,
            Health::Dormant => VITALITY_DORMANT,
        }
    }

    /// Forgiving parse from a wire string. Returns `None` for unrecognized
    /// input so callers can choose their own default (typically `Dormant`
    /// for "we don't know, so assume offline").
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            s if s == VITALITY_THRIVING => Some(Health::Thriving),
            s if s == VITALITY_NEEDS_ATTENTION => Some(Health::NeedsAttention),
            s if s == VITALITY_WITHERING => Some(Health::Withering),
            s if s == VITALITY_WILTING => Some(Health::Wilting),
            s if s == VITALITY_DORMANT => Some(Health::Dormant),
            _ => None,
        }
    }

    /// True only for [`Health::Thriving`]. "Is this stone fully OK?"
    pub fn is_ok(&self) -> bool {
        matches!(self, Health::Thriving)
    }

    /// True for anything that isn't [`Health::Thriving`].
    pub fn needs_attention(&self) -> bool {
        !self.is_ok()
    }

    /// True for [`Health::Wilting`]. "Is this stone in terminal state?"
    pub fn is_terminal(&self) -> bool {
        matches!(self, Health::Wilting)
    }

    /// True for [`Health::Dormant`]. "Is this stone unreachable?"
    pub fn is_dormant(&self) -> bool {
        matches!(self, Health::Dormant)
    }
}

impl std::fmt::Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Health> for &'static str {
    fn from(value: Health) -> Self {
        value.as_str()
    }
}

impl From<Health> for String {
    fn from(value: Health) -> Self {
        value.as_str().to_string()
    }
}

impl Default for Health {
    /// Default is [`Health::Dormant`] — safest assumption when we don't
    /// have a known value (e.g. fresh `GardenState` before the first
    /// presence event arrives).
    fn default() -> Self {
        Health::Dormant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_vitality_constants() {
        assert_eq!(Health::Thriving.as_str(), VITALITY_THRIVING);
        assert_eq!(Health::NeedsAttention.as_str(), VITALITY_NEEDS_ATTENTION);
        assert_eq!(Health::Withering.as_str(), VITALITY_WITHERING);
        assert_eq!(Health::Wilting.as_str(), VITALITY_WILTING);
        assert_eq!(Health::Dormant.as_str(), VITALITY_DORMANT);
    }

    #[test]
    fn parse_accepts_all_canonical_strings() {
        assert_eq!(Health::parse("thriving"), Some(Health::Thriving));
        assert_eq!(
            Health::parse("needs attention"),
            Some(Health::NeedsAttention)
        );
        assert_eq!(Health::parse("withering"), Some(Health::Withering));
        assert_eq!(Health::parse("wilting"), Some(Health::Wilting));
        assert_eq!(Health::parse("dormant"), Some(Health::Dormant));
    }

    #[test]
    fn parse_returns_none_for_unknown() {
        assert_eq!(Health::parse("unknown"), None);
        assert_eq!(Health::parse(""), None);
        assert_eq!(Health::parse("THRIVING"), None); // case-sensitive per wire convention
    }

    #[test]
    fn serde_round_trips_through_json() {
        for h in [
            Health::Thriving,
            Health::NeedsAttention,
            Health::Withering,
            Health::Wilting,
            Health::Dormant,
        ] {
            let json = serde_json::to_string(&h).unwrap();
            // Wire form is the bare string, quoted
            assert_eq!(json, format!("\"{}\"", h.as_str()));
            let back: Health = serde_json::from_str(&json).unwrap();
            assert_eq!(back, h);
        }
    }

    #[test]
    fn is_ok_only_for_thriving() {
        assert!(Health::Thriving.is_ok());
        assert!(!Health::NeedsAttention.is_ok());
        assert!(!Health::Withering.is_ok());
        assert!(!Health::Wilting.is_ok());
        assert!(!Health::Dormant.is_ok());
    }

    #[test]
    fn needs_attention_inverse_of_is_ok() {
        for h in [
            Health::Thriving,
            Health::NeedsAttention,
            Health::Withering,
            Health::Wilting,
            Health::Dormant,
        ] {
            assert_eq!(h.needs_attention(), !h.is_ok());
        }
    }

    #[test]
    fn is_terminal_only_for_wilting() {
        assert!(Health::Wilting.is_terminal());
        assert!(!Health::Thriving.is_terminal());
        assert!(!Health::Withering.is_terminal());
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", Health::Thriving), "thriving");
        assert_eq!(format!("{}", Health::NeedsAttention), "needs attention");
        assert_eq!(format!("{}", Health::Wilting), "wilting");
    }

    #[test]
    fn conversions_to_string_and_static_str() {
        let static_str: &'static str = Health::Thriving.into();
        assert_eq!(static_str, "thriving");

        let owned: String = Health::Wilting.into();
        assert_eq!(owned, "wilting");
    }
}
