//! Skill monikers — lowercase kebab-case identifiers unique within a primitive.
//!
//! A [`Moniker`] is a human-readable skill name like `"outpaint"`,
//! `"vision-tag"`, or `"cute-bunny-lora"`. It appears:
//!
//! - in URLs: `/v1/image/generate/outpaint`
//! - in catalog listings
//! - on disk: `{data_dir}/skills/{provider}/{moniker}/`
//!
//! Constraints enforced at construction time:
//!
//! - 1..=64 characters
//! - lowercase ASCII letters, digits, and `-` only
//! - first character is a letter (no leading digit or dash)
//! - last character is a letter or digit (no trailing dash)
//! - no double dashes (`--`)
//! - not in the reserved-word list (names that would collide with URL
//!   path segments or meta-action identifiers)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Reserved monikers. Any attempt to create a moniker matching one of
/// these is rejected, because the name is already in use as a URL path
/// segment or meta-action identifier elsewhere in the surface.
pub const RESERVED: &[&str] = &[
    "new",
    "list",
    "batch",
    "run",
    "schema",
    "catalog",
    "providers",
    "do",
    "media",
    "jobs",
    "events",
    "health",
    "recommendations",
    "flush",
    // Prevent collision with modality segments
    "text",
    "image",
    "audio",
    "video",
    // Prevent collision with primitive leaves
    "chat",
    "translate",
    "embed",
    "rerank",
    "generate",
    "edit",
    "upscale",
    "analyze",
    "transcribe",
];

/// Maximum length of a moniker in characters.
pub const MAX_LENGTH: usize = 64;

/// Skill moniker value object.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Moniker(String);

impl Moniker {
    /// Construct and validate a moniker.
    pub fn new(value: impl Into<String>) -> Result<Self, MonikerError> {
        let s: String = value.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Validate a candidate moniker string without allocating.
    pub fn validate(s: &str) -> Result<(), MonikerError> {
        if s.is_empty() {
            return Err(MonikerError::Empty);
        }
        if s.chars().count() > MAX_LENGTH {
            return Err(MonikerError::TooLong {
                length: s.chars().count(),
                max: MAX_LENGTH,
            });
        }
        let bytes = s.as_bytes();
        let first = bytes[0];
        if !first.is_ascii_lowercase() {
            return Err(MonikerError::InvalidStart(s.to_string()));
        }
        let last = bytes[bytes.len() - 1];
        if !(last.is_ascii_lowercase() || last.is_ascii_digit()) {
            return Err(MonikerError::InvalidEnd(s.to_string()));
        }
        for (idx, byte) in bytes.iter().enumerate() {
            match byte {
                b'a'..=b'z' | b'0'..=b'9' => {}
                b'-' => {
                    if idx > 0 && bytes[idx - 1] == b'-' {
                        return Err(MonikerError::DoubleDash(s.to_string()));
                    }
                }
                _ => {
                    return Err(MonikerError::InvalidCharacter {
                        moniker: s.to_string(),
                        byte: *byte as char,
                    });
                }
            }
        }
        if RESERVED.iter().any(|r| *r == s) {
            return Err(MonikerError::Reserved(s.to_string()));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Moniker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Moniker {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Moniker {
    type Error = MonikerError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Moniker> for String {
    fn from(value: Moniker) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MonikerError {
    #[error("moniker is empty")]
    Empty,
    #[error("moniker `{0}` must start with a lowercase ASCII letter")]
    InvalidStart(String),
    #[error("moniker `{0}` must end with a lowercase ASCII letter or digit")]
    InvalidEnd(String),
    #[error("moniker is {length} characters; maximum is {max}")]
    TooLong { length: usize, max: usize },
    #[error("moniker `{moniker}` contains invalid character `{byte}` (only [a-z0-9-] allowed)")]
    InvalidCharacter { moniker: String, byte: char },
    #[error("moniker `{0}` contains `--` which is not allowed")]
    DoubleDash(String),
    #[error("moniker `{0}` is reserved and cannot be used as a skill name")]
    Reserved(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_monikers() {
        assert!(Moniker::new("outpaint").is_ok());
        assert!(Moniker::new("vision-tag").is_ok());
        assert!(Moniker::new("cute-bunny-lora").is_ok());
        assert!(Moniker::new("sd15").is_ok());
        assert!(Moniker::new("v1-beta").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Moniker::new(""), Err(MonikerError::Empty));
    }

    #[test]
    fn rejects_uppercase() {
        assert!(matches!(
            Moniker::new("Outpaint"),
            Err(MonikerError::InvalidStart(_))
        ));
    }

    #[test]
    fn rejects_leading_dash() {
        assert!(matches!(
            Moniker::new("-outpaint"),
            Err(MonikerError::InvalidStart(_))
        ));
    }

    #[test]
    fn rejects_trailing_dash() {
        assert!(matches!(
            Moniker::new("outpaint-"),
            Err(MonikerError::InvalidEnd(_))
        ));
    }

    #[test]
    fn rejects_double_dash() {
        assert!(matches!(
            Moniker::new("out--paint"),
            Err(MonikerError::DoubleDash(_))
        ));
    }

    #[test]
    fn rejects_too_long() {
        let long = "a".repeat(MAX_LENGTH + 1);
        assert!(matches!(
            Moniker::new(long),
            Err(MonikerError::TooLong { .. })
        ));
    }

    #[test]
    fn rejects_reserved_words() {
        assert!(matches!(
            Moniker::new("new"),
            Err(MonikerError::Reserved(_))
        ));
        assert!(matches!(
            Moniker::new("batch"),
            Err(MonikerError::Reserved(_))
        ));
        assert!(matches!(
            Moniker::new("chat"),
            Err(MonikerError::Reserved(_))
        ));
    }

    #[test]
    fn rejects_special_characters() {
        assert!(Moniker::new("out_paint").is_err());
        assert!(Moniker::new("out.paint").is_err());
        assert!(Moniker::new("out paint").is_err());
    }
}
