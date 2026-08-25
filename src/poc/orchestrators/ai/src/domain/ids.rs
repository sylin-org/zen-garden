//! Identity types for the orchestrator.
//!
//! Mutable identities — request, response, media, job, registration — are
//! GUIDv7. GUIDv7 is lexicographically sortable, time-ordered, and opaque
//! to external callers.
//!
//! Human-readable identities — provider name, primitive, moniker — are
//! plain strings wrapped in newtypes so the compiler can distinguish them
//! at call sites.
//!
//! Correlation IDs are GUIDv7 when synthesized by the orchestrator; when
//! the caller supplies `X-Correlation-Id` (or derives one from a
//! W3C `traceparent` header) the raw string is preserved to respect
//! upstream identity.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use garden_common::utils::ids::generate_guidv7;

/// Macro defining a GUIDv7-backed identity type.
macro_rules! guid_id {
    ($(#[$meta:meta])* $vis:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        $vis struct $name(String);

        impl $name {
            /// Generate a fresh GUIDv7 identifier.
            pub fn generate() -> Self {
                Self(generate_guidv7())
            }

            /// Wrap an externally-supplied value without validation.
            /// The caller is responsible for ensuring the value is well
            /// formed (used by deserializers and tests).
            pub fn from_string(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

guid_id!(
    /// Per-request GUIDv7.
    pub RequestId
);

guid_id!(
    /// Per-response GUIDv7 (assigned when the response leaves the orchestrator).
    pub ResponseId
);

guid_id!(
    /// Media GUIDv7 — assigned on upload or on `open_writer` for streaming
    /// media. Stable for the lifetime of the entry.
    pub MediaId
);

guid_id!(
    /// Job GUIDv7.
    pub JobId
);

guid_id!(
    /// Pipeline instance GUIDv7. Reserved for future pipelines.
    pub PipelineRunId
);

/// Correlation identifier. Preserved verbatim when supplied by the caller;
/// synthesized as a GUIDv7 when absent. Shared ownership via `Arc<str>` so
/// clones are cheap even in long-running requests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(Arc<str>);

impl Serialize for CorrelationId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CorrelationId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self(Arc::from(s)))
    }
}

impl CorrelationId {
    /// Generate a fresh correlation id (GUIDv7).
    pub fn generate() -> Self {
        Self(Arc::from(generate_guidv7()))
    }

    /// Wrap an externally-supplied id.
    pub fn from_string(value: impl Into<String>) -> Self {
        Self(Arc::from(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CorrelationId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Human-readable provider identity (e.g., `"ollama"`, `"anthropic"`).
///
/// `ProviderName` values are lowercase ASCII snake-case strings, chosen
/// by the adapter at construction. They appear in URLs, logs, metrics,
/// and response metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderName(String);

impl ProviderName {
    /// Construct a [`ProviderName`] from a runtime string.
    ///
    /// Panics on empty input; this is only reachable from adapter
    /// constructors where the name is a compile-time constant, so a panic
    /// is the correct response (the adapter would never have compiled).
    pub fn new(value: impl Into<String>) -> Self {
        let s: String = value.into();
        assert!(!s.is_empty(), "provider name must not be empty");
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ProviderName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ProviderName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ProviderName {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

// `ModelFqn` and `RegistrationId` previously lived here. They were
// removed in ORCH-0030 R2 M3 along with the legacy `Directory`
// aggregate and the `Registration` provider type — model resolution
// is now adapter-local and there is no need for cross-provider
// fully-qualified model names.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_ids_are_unique() {
        let a = RequestId::generate();
        let b = RequestId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn correlation_id_preserves_external_value() {
        let cid = CorrelationId::from_string("req-abc-123");
        assert_eq!(cid.as_str(), "req-abc-123");
    }
}
