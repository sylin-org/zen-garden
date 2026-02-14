//! Offering Search Types
//!
//! Shared types for offering search between Moss and Rake.
//! Moss performs all search logic; Rake is a thin client.

use serde::{Deserialize, Serialize};

// ============================================================================
// Taxonomy Dictionary
// ============================================================================

/// Taxonomy dictionary for normalizing search tokens.
/// Maps user intent to canonical terms (e.g., "nosql" → "mongodb").
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TaxonomyDictionary {
    #[serde(default)]
    pub map: std::collections::HashMap<String, String>,
}

// ============================================================================
// Search Request/Response
// ============================================================================

/// Request to search offerings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfferingSearchRequest {
    /// Free-form search query (e.g., "nosql database", "vector store")
    pub query: String,
    /// Optional hardware preferences (e.g., ["ssd", "nvme"])
    #[serde(default)]
    pub prefer: Vec<String>,
    /// Maximum results to return (default: 5)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    5
}

/// A single offering search result.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfferingSearchResult {
    /// Offering name
    pub name: String,
    /// Category (e.g., "data", "cache")
    pub category: String,
    /// Description
    pub description: String,
    /// Tags for this offering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Container image
    pub image: String,
    /// Relevance score (higher = more relevant)
    pub score: i32,
    /// Compatibility decision ("native", "fallback", "fail")
    pub compatibility: String,
    /// Optional compatibility reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_reason: Option<String>,
}

/// Response from offering search.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfferingSearchResponse {
    /// Search query (echoed back)
    pub query: String,
    /// Normalized tokens used for matching
    pub tokens: Vec<String>,
    /// Matched offerings sorted by relevance
    pub results: Vec<OfferingSearchResult>,
    /// Total available offerings (before filtering)
    pub total_offerings: usize,
}

// ============================================================================
// Garden-wide Search (across stones)
// ============================================================================

/// Result from garden-wide offering search (includes stone info).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GardenOfferingSearchResult {
    /// Stone name where this offering is available
    pub stone_name: String,
    /// Stone endpoint
    pub stone_endpoint: String,
    /// The offering details
    #[serde(flatten)]
    pub offering: OfferingSearchResult,
    /// Combined score (relevance + stone preference)
    pub combined_score: i32,
}

/// Response from garden-wide offering search.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GardenOfferingSearchResponse {
    /// Search query (echoed back)
    pub query: String,
    /// Normalized tokens used for matching
    pub tokens: Vec<String>,
    /// Matched offerings from all stones, sorted by combined score
    pub results: Vec<GardenOfferingSearchResult>,
    /// Number of stones queried
    pub stones_queried: usize,
}

// ============================================================================
// Offering Instance Names (FQN)
// ============================================================================

/// Offering fully-qualified name (FQN) with optional instance suffix.
///
/// Examples:
/// - "ollama" (default instance)
/// - "ollama:dev" (named instance)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferingFqn {
    /// Offering template/type name (e.g., "ollama")
    pub offering: String,
    /// Optional instance name (e.g., "dev")
    pub instance: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OfferingFqnError {
    pub message: String,
}

impl std::fmt::Display for OfferingFqnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OfferingFqnError {}

impl OfferingFqn {
    /// Return the normalized fully-qualified name.
    pub fn fqn(&self) -> String {
        match &self.instance {
            Some(instance) => format!(
                "{}{}{}",
                self.offering,
                crate::constants::OFFERING_FQN_SEPARATOR,
                instance
            ),
            None => self.offering.clone(),
        }
    }

    /// Return the instance name if present, otherwise the offering name.
    pub fn instance_or_offering(&self) -> &str {
        self.instance.as_deref().unwrap_or(&self.offering)
    }

    /// Encode the FQN for container and filesystem identifiers.
    pub fn encoded_for_container(&self) -> String {
        match &self.instance {
            Some(instance) => format!(
                "{}{}{}",
                self.offering,
                crate::constants::OFFERING_FQN_CONTAINER_SEPARATOR,
                instance
            ),
            None => self.offering.clone(),
        }
    }
}

/// Parse and validate an offering FQN string.
///
/// Rules:
/// - Format: `offering[:instance]`
/// - Lowercase, trimmed
/// - Allowed characters: `[a-z0-9_-]` in each segment
/// - Segments must start with a letter
/// - `--` is reserved (used for container and filesystem encoding)
pub fn parse_offering_fqn(input: &str) -> Result<OfferingFqn, OfferingFqnError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(OfferingFqnError {
            message: "Offering name cannot be empty".to_string(),
        });
    }

    let mut parts = trimmed.split(crate::constants::OFFERING_FQN_SEPARATOR);
    let offering_raw = parts.next().unwrap_or_default();
    let instance_raw = parts.next();
    if parts.next().is_some() {
        return Err(OfferingFqnError {
            message: "Offering name cannot contain multiple ':' separators".to_string(),
        });
    }

    let offering = normalize_fqn_segment(offering_raw, "offering")?;
    let instance = match instance_raw {
        Some(raw) => {
            let normalized = normalize_fqn_segment(raw, "instance")?;
            if normalized == offering {
                None
            } else {
                Some(normalized)
            }
        }
        None => None,
    };

    Ok(OfferingFqn { offering, instance })
}

fn normalize_fqn_segment(segment: &str, label: &str) -> Result<String, OfferingFqnError> {
    let normalized = segment.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(OfferingFqnError {
            message: format!("{} segment cannot be empty", label),
        });
    }

    if normalized.len() > crate::api_utils::MAX_NAME_LENGTH {
        return Err(OfferingFqnError {
            message: format!("{} segment exceeds maximum length", label),
        });
    }

    if normalized.starts_with('-') || normalized.starts_with('_') {
        return Err(OfferingFqnError {
            message: format!("{} segment cannot start with '-' or '_'", label),
        });
    }

    if !normalized
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        return Err(OfferingFqnError {
            message: format!("{} segment must start with a letter", label),
        });
    }

    if normalized.contains(crate::constants::OFFERING_FQN_CONTAINER_SEPARATOR) {
        return Err(OfferingFqnError {
            message: format!(
                "{} segment cannot contain '{}'",
                label,
                crate::constants::OFFERING_FQN_CONTAINER_SEPARATOR
            ),
        });
    }

    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(OfferingFqnError {
            message: format!(
                "{} segment contains invalid characters (allowed: a-z, 0-9, _, -)",
                label
            ),
        });
    }

    Ok(normalized)
}

#[cfg(test)]
mod fqn_tests {
    use super::*;

    #[test]
    fn parse_offering_fqn_default() {
        let fqn = parse_offering_fqn("ollama").unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert!(fqn.instance.is_none());
        assert_eq!(fqn.fqn(), "ollama");
    }

    #[test]
    fn parse_offering_fqn_instance() {
        let fqn = parse_offering_fqn("ollama:dev").unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("dev"));
        assert_eq!(fqn.fqn(), "ollama:dev");
    }

    #[test]
    fn parse_offering_fqn_normalizes_case() {
        let fqn = parse_offering_fqn("Ollama:DEV").unwrap();
        assert_eq!(fqn.fqn(), "ollama:dev");
    }

    #[test]
    fn parse_offering_fqn_rejects_multiple_colons() {
        let err = parse_offering_fqn("ollama:dev:extra").unwrap_err();
        assert!(err.message.contains("multiple"));
    }

    #[test]
    fn parse_offering_fqn_rejects_invalid_chars() {
        let err = parse_offering_fqn("olla$ma").unwrap_err();
        assert!(err.message.contains("invalid characters"));
    }

    #[test]
    fn parse_offering_fqn_rejects_reserved_separator() {
        let err = parse_offering_fqn("olla--ma").unwrap_err();
        assert!(err.message.contains("cannot contain"));
    }
}
