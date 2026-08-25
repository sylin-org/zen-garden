//! `FieldPath` — the canonical identifier for a field in a canonical payload.
//!
//! A field path is a dotted string like `text.prompt.user` or
//! `usage.tokens.input`. Every canonical field key used at runtime is a
//! constant [`FieldPath`] declared in [`crate::domain::keys`]. Constructors
//! outside that module are reserved for deserialization paths (e.g.,
//! parsing aliases supplied by callers) and tests.
//!
//! The type is cheap to copy — it wraps either a `&'static str` (for
//! constants) or an `Arc<str>` (for runtime-created paths such as aliases).
//! Equality and ordering are performed on the underlying string so a
//! constant and a parsed copy compare equal.
//!
//! Nested JSON serialization: [`crate::domain::output::Output`] splits on
//! `.` to build nested `serde_json::Value` objects. Keys containing a `.`
//! are therefore invalid and rejected by [`FieldPath::validate`].

use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A dotted canonical field path.
///
/// Prefer the constants in [`crate::domain::keys`]. Use [`FieldPath::parse`]
/// only at system boundaries where the input is not a known constant
/// (alias sources, user-supplied field selectors, etc.).
#[derive(Clone)]
pub struct FieldPath {
    inner: Repr,
}

#[derive(Clone)]
enum Repr {
    Static(&'static str),
    Owned(Arc<str>),
}

impl FieldPath {
    /// Construct a [`FieldPath`] from a `&'static str`. The input is not
    /// validated at compile time; the intended use is `const` declarations
    /// in [`crate::domain::keys`] where the author has already checked the
    /// syntax. Runtime paths should go through [`FieldPath::parse`].
    pub const fn new(s: &'static str) -> Self {
        Self { inner: Repr::Static(s) }
    }

    /// Parse and validate a runtime string into a [`FieldPath`].
    ///
    /// Rejected:
    /// - empty strings
    /// - segments that are empty (e.g. `"text..user"`)
    /// - segments beginning with a digit (`"text.1.user"`)
    /// - segments containing characters outside `[a-z0-9_]`
    /// - leading/trailing `.`
    pub fn parse(input: &str) -> Result<Self, FieldPathError> {
        Self::validate(input)?;
        Ok(Self {
            inner: Repr::Owned(Arc::from(input)),
        })
    }

    /// Validate the shape of a dotted field path without allocating.
    pub fn validate(input: &str) -> Result<(), FieldPathError> {
        if input.is_empty() {
            return Err(FieldPathError::Empty);
        }
        if input.starts_with('.') || input.ends_with('.') {
            return Err(FieldPathError::LeadingOrTrailingDot);
        }
        for segment in input.split('.') {
            if segment.is_empty() {
                return Err(FieldPathError::EmptySegment);
            }
            let first = segment.as_bytes()[0];
            if !(first.is_ascii_lowercase() || first == b'_') {
                return Err(FieldPathError::InvalidStart(segment.to_string()));
            }
            for byte in segment.bytes() {
                if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') {
                    return Err(FieldPathError::InvalidCharacter {
                        segment: segment.to_string(),
                        byte: byte as char,
                    });
                }
            }
        }
        Ok(())
    }

    /// Borrow the underlying dotted representation.
    pub fn as_str(&self) -> &str {
        match &self.inner {
            Repr::Static(s) => s,
            Repr::Owned(s) => s,
        }
    }

    /// The top-level namespace segment (e.g. `"text"` in `"text.prompt.user"`).
    pub fn namespace(&self) -> &str {
        self.as_str().split('.').next().unwrap_or("")
    }

    /// Iterate over dotted segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.as_str().split('.')
    }

    /// Returns `true` if this path is under the given namespace segment.
    pub fn is_in_namespace(&self, namespace: &str) -> bool {
        self.namespace() == namespace
    }

    /// Returns `true` for user-defined passthrough paths (prefix `x_`).
    /// Passthrough keys bypass vocabulary validation and are forwarded
    /// to providers as-is.
    pub fn is_passthrough(&self) -> bool {
        self.as_str().starts_with("x_")
            || self
                .as_str()
                .split('.')
                .any(|segment| segment.starts_with("x_"))
    }
}

impl fmt::Debug for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldPath({})", self.as_str())
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq for FieldPath {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for FieldPath {}

impl PartialOrd for FieldPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FieldPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for FieldPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl AsRef<str> for FieldPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&FieldPath> for FieldPath {
    fn from(value: &FieldPath) -> Self {
        value.clone()
    }
}

impl<'a> From<&'a FieldPath> for Cow<'a, str> {
    fn from(value: &'a FieldPath) -> Self {
        Cow::Borrowed(value.as_str())
    }
}

impl Serialize for FieldPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FieldPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        FieldPath::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FieldPathError {
    #[error("field path is empty")]
    Empty,
    #[error("field path has a leading or trailing `.`")]
    LeadingOrTrailingDot,
    #[error("field path contains an empty segment")]
    EmptySegment,
    #[error("segment `{0}` must start with a lowercase ASCII letter or underscore")]
    InvalidStart(String),
    #[error("segment `{segment}` contains invalid character `{byte}` (only [a-z0-9_] allowed)")]
    InvalidCharacter { segment: String, byte: char },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_paths() {
        assert!(FieldPath::parse("text.prompt.user").is_ok());
        assert!(FieldPath::parse("usage.tokens.input").is_ok());
        assert!(FieldPath::parse("image.dimensions.width").is_ok());
        assert!(FieldPath::parse("x_custom_namespace.field").is_ok());
        assert!(FieldPath::parse("a").is_ok());
        assert!(FieldPath::parse("_underscore_leading").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(FieldPath::parse(""), Err(FieldPathError::Empty));
    }

    #[test]
    fn rejects_leading_trailing_dots() {
        assert!(matches!(
            FieldPath::parse(".text.prompt"),
            Err(FieldPathError::LeadingOrTrailingDot)
        ));
        assert!(matches!(
            FieldPath::parse("text.prompt."),
            Err(FieldPathError::LeadingOrTrailingDot)
        ));
    }

    #[test]
    fn rejects_empty_segments() {
        assert!(matches!(
            FieldPath::parse("text..prompt"),
            Err(FieldPathError::EmptySegment)
        ));
    }

    #[test]
    fn rejects_digit_start() {
        assert!(matches!(
            FieldPath::parse("text.1prompt"),
            Err(FieldPathError::InvalidStart(_))
        ));
    }

    #[test]
    fn rejects_uppercase() {
        assert!(matches!(
            FieldPath::parse("text.Prompt"),
            Err(FieldPathError::InvalidStart(_))
        ));
        assert!(matches!(
            FieldPath::parse("text.promPt"),
            Err(FieldPathError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn static_and_parsed_compare_equal() {
        const STATIC: FieldPath = FieldPath::new("text.prompt.user");
        let parsed = FieldPath::parse("text.prompt.user").unwrap();
        assert_eq!(STATIC, parsed);
    }

    #[test]
    fn namespace_is_first_segment() {
        let path = FieldPath::parse("text.prompt.user").unwrap();
        assert_eq!(path.namespace(), "text");
        assert!(path.is_in_namespace("text"));
        assert!(!path.is_in_namespace("image"));
    }

    #[test]
    fn passthrough_detection() {
        assert!(FieldPath::parse("x_custom.field").unwrap().is_passthrough());
        assert!(FieldPath::parse("text.x_custom").unwrap().is_passthrough());
        assert!(!FieldPath::parse("text.prompt").unwrap().is_passthrough());
    }

    #[test]
    fn serde_roundtrip() {
        let path = FieldPath::parse("text.prompt.user").unwrap();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, "\"text.prompt.user\"");
        let decoded: FieldPath = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, path);
    }
}
