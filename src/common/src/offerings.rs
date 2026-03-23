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
// Offering Source Schemes (OFFER-0006)
// ============================================================================

/// Source scheme determining how an offering is resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OfferingSource {
    /// Docker registry image inspection (`image:nginx:latest`).
    Image,
    /// Remote manifest repository (`repo:community/bookstack`).
    Repo(String),
    /// OCI artifact pull (`oci:ghcr.io/zen/manifests/bookstack:1.0`), future.
    Oci,
}

// ============================================================================

// ============================================================================
// Offering FQN (OFFER-0006: FQN v2)
// ============================================================================

/// Offering fully-qualified name — the single currency for offering identity.
///
/// Grammar: `[source ":"] name ["::" instance]`
///
/// Examples:
/// - `"mongodb"` — curated offering, default instance
/// - `"mongodb::prod"` — curated offering, named instance
/// - `"image:nginx:latest"` — image-direct, default instance
/// - `"image:nginx:latest::staging"` — image-direct, named instance
/// - `"repo:community/bookstack"` — from remote manifest repo
///
/// Serializes as a plain FQN string in JSON for backward-compatible persistence.
/// Deserializes with automatic legacy normalization (`@` → `::`, `:` → `::`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OfferingFqn {
    /// Source scheme (None for curated/embedded offerings).
    pub source: Option<OfferingSource>,
    /// Offering template/type name (e.g., `"ollama"`, `"nginx"`).
    pub offering: String,
    /// Optional instance name (e.g., `"dev"`, `"prod"`, `"adopted"`).
    pub instance: Option<String>,
    /// Raw Docker image reference for image-direct offerings (e.g., `"nginx:latest"`).
    pub image_ref: Option<String>,
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

// ── Display ────────────────────────────────────────────────────────────────

impl std::fmt::Display for OfferingFqn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fqn())
    }
}

// ── Serde ──────────────────────────────────────────────────────────────────

impl serde::Serialize for OfferingFqn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.fqn())
    }
}

impl<'de> serde::Deserialize<'de> for OfferingFqn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        OfferingFqn::parse(&s).map_err(serde::de::Error::custom)
    }
}

// ── Builders ───────────────────────────────────────────────────────────────

impl OfferingFqn {
    /// Create a curated offering FQN with default instance.
    pub fn new(offering: &str) -> Result<Self, OfferingFqnError> {
        let offering = validate_fqn_segment(offering, "offering")?;
        Ok(Self {
            source: None,
            offering,
            instance: None,
            image_ref: None,
        })
    }

    /// Create a curated offering FQN with a named instance.
    pub fn with_instance(offering: &str, instance: &str) -> Result<Self, OfferingFqnError> {
        let offering = validate_fqn_segment(offering, "offering")?;
        let instance = validate_fqn_segment(instance, "instance")?;
        let instance = if instance == offering {
            None
        } else {
            Some(instance)
        };
        Ok(Self {
            source: None,
            offering,
            instance,
            image_ref: None,
        })
    }

    /// Create an adopted offering FQN (`offering::adopted`).
    pub fn adopted(offering: &str) -> Result<Self, OfferingFqnError> {
        Self::with_instance(offering, crate::constants::OFFERING_ADOPTED_INSTANCE)
    }

    /// Create an image-direct FQN with default instance.
    pub fn image_direct(image_ref: &str) -> Result<Self, OfferingFqnError> {
        let image_ref = image_ref.trim();
        if image_ref.is_empty() {
            return Err(OfferingFqnError {
                message: "Image reference cannot be empty".to_string(),
            });
        }
        let offering = offering_name_from_image_ref(image_ref);
        Ok(Self {
            source: Some(OfferingSource::Image),
            offering,
            instance: None,
            image_ref: Some(image_ref.to_string()),
        })
    }

    /// Create an image-direct FQN with a named instance.
    pub fn image_direct_with_instance(
        image_ref: &str,
        instance: &str,
    ) -> Result<Self, OfferingFqnError> {
        let mut fqn = Self::image_direct(image_ref)?;
        let instance = validate_fqn_segment(instance, "instance")?;
        fqn.instance = Some(instance);
        Ok(fqn)
    }
}

// ── Core Methods ───────────────────────────────────────────────────────────

impl OfferingFqn {
    /// Return the canonical FQN string: `[source:]name[::instance]`.
    pub fn fqn(&self) -> String {
        let mut s = String::new();

        match &self.source {
            Some(OfferingSource::Image) => {
                s.push_str("image:");
                s.push_str(self.image_ref.as_deref().unwrap_or(&self.offering));
            }
            Some(OfferingSource::Repo(repo)) => {
                s.push_str("repo:");
                s.push_str(repo);
                s.push('/');
                s.push_str(&self.offering);
            }
            Some(OfferingSource::Oci) => {
                s.push_str("oci:");
                s.push_str(&self.offering);
            }
            None => {
                s.push_str(&self.offering);
            }
        }

        if let Some(instance) = &self.instance {
            s.push_str(crate::constants::OFFERING_FQN_SEPARATOR);
            s.push_str(instance);
        }

        s
    }

    /// Compare the FQN to a string without allocating.
    ///
    /// For the common case (no source scheme, no instance), this is a direct
    /// `&str` comparison with zero allocation. For offerings with a source
    /// scheme or instance suffix, falls back to constructing the FQN string.
    pub fn fqn_eq(&self, other: &str) -> bool {
        // Fast path: curated offering, default instance → FQN is just the offering name
        if self.source.is_none() && self.instance.is_none() {
            return self.offering == other;
        }
        // Slow path: construct the full FQN string for comparison
        self.fqn() == other
    }

    /// Return the instance name if present, otherwise the offering name.
    pub fn instance_or_offering(&self) -> &str {
        self.instance.as_deref().unwrap_or(&self.offering)
    }

    /// Whether this is an image-direct offering.
    pub fn is_image_direct(&self) -> bool {
        matches!(self.source, Some(OfferingSource::Image))
    }

    /// Whether this is a curated (no source scheme) offering.
    pub fn is_curated(&self) -> bool {
        self.source.is_none()
    }

    /// Encode the FQN for container names and filesystem identifiers.
    ///
    /// Curated: `mongodb--prod` (unchanged from V1).
    /// Image-direct: `img-nginx-latest--staging` (new).
    pub fn encoded_for_container(&self) -> String {
        match &self.source {
            Some(OfferingSource::Image) => {
                let sanitized = sanitize_image_ref_for_container(
                    self.image_ref.as_deref().unwrap_or(&self.offering),
                );
                match &self.instance {
                    Some(inst) => format!(
                        "img-{}{}{}",
                        sanitized,
                        crate::constants::OFFERING_FQN_CONTAINER_SEPARATOR,
                        inst
                    ),
                    None => format!("img-{}", sanitized),
                }
            }
            _ => match &self.instance {
                Some(instance) => format!(
                    "{}{}{}",
                    self.offering,
                    crate::constants::OFFERING_FQN_CONTAINER_SEPARATOR,
                    instance
                ),
                None => self.offering.clone(),
            },
        }
    }
}

// ── Parser ─────────────────────────────────────────────────────────────────

/// Known source scheme prefixes.
const SOURCE_SCHEMES: &[&str] = &["image:", "repo:", "oci:"];

impl OfferingFqn {
    /// Parse an FQN string with automatic legacy normalization.
    ///
    /// Handles V2 (`mongodb::prod`), V1 legacy (`mongodb:prod`),
    /// V0 legacy (`mongodb@prod`), and source schemes (`image:nginx:latest`).
    pub fn parse(input: &str) -> Result<Self, OfferingFqnError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(OfferingFqnError {
                message: "Offering name cannot be empty".to_string(),
            });
        }

        // V0 legacy: @ → ::
        if trimmed.contains('@') {
            let migrated = trimmed.replace('@', "::");
            return Self::parse_v2(&migrated);
        }

        // Source scheme detection: check known prefixes
        for &scheme in SOURCE_SCHEMES {
            if trimmed.starts_with(scheme) {
                return Self::parse_with_source(trimmed, scheme);
            }
        }

        // V2 native: contains ::
        if trimmed.contains("::") {
            return Self::parse_v2(trimmed);
        }

        // V1 legacy: contains single : but no :: and no source scheme
        // A bare single-colon like "mongodb:prod" is V1 format → normalize to V2
        if trimmed.contains(':') {
            let migrated = trimmed.replacen(':', "::", 1);
            return Self::parse_v2(&migrated);
        }

        // Plain offering name (no separator, no source)
        Self::parse_v2(trimmed)
    }

    /// Parse a V2-format string: `offering[::instance]`.
    fn parse_v2(input: &str) -> Result<Self, OfferingFqnError> {
        let (offering_raw, instance_raw) = match input.split_once("::") {
            Some((left, right)) => {
                // Reject multiple :: separators
                if right.contains("::") {
                    return Err(OfferingFqnError {
                        message: "FQN cannot contain multiple '::' separators".to_string(),
                    });
                }
                (left, Some(right))
            }
            None => (input, None),
        };

        let offering = validate_fqn_segment(offering_raw, "offering")?;
        let instance = match instance_raw {
            Some(raw) => {
                let normalized = validate_fqn_segment(raw, "instance")?;
                if normalized == offering {
                    None // Canonicalize: mongodb::mongodb → mongodb
                } else {
                    Some(normalized)
                }
            }
            None => None,
        };

        Ok(Self {
            source: None,
            offering,
            instance,
            image_ref: None,
        })
    }

    /// Parse an FQN with a known source scheme prefix.
    fn parse_with_source(input: &str, scheme: &str) -> Result<Self, OfferingFqnError> {
        let after_scheme = &input[scheme.len()..];
        if after_scheme.is_empty() {
            return Err(OfferingFqnError {
                message: format!(
                    "Missing value after '{}' source scheme",
                    scheme.trim_end_matches(':')
                ),
            });
        }

        // Split instance first: everything after :: is instance
        let (name_part, instance_raw) = match after_scheme.split_once("::") {
            Some((left, right)) => {
                if right.contains("::") {
                    return Err(OfferingFqnError {
                        message: "FQN cannot contain multiple '::' separators".to_string(),
                    });
                }
                (left, Some(right))
            }
            None => (after_scheme, None),
        };

        let instance = match instance_raw {
            Some(raw) => Some(validate_fqn_segment(raw, "instance")?),
            None => None,
        };

        match scheme {
            "image:" => {
                let image_ref = name_part.trim();
                if image_ref.is_empty() {
                    return Err(OfferingFqnError {
                        message: "Image reference cannot be empty".to_string(),
                    });
                }
                let offering = offering_name_from_image_ref(image_ref);
                Ok(Self {
                    source: Some(OfferingSource::Image),
                    offering,
                    instance,
                    image_ref: Some(image_ref.to_string()),
                })
            }
            "repo:" => {
                // repo:community/bookstack → repo="community", offering="bookstack"
                let (repo, offering_name) =
                    name_part.rsplit_once('/').ok_or_else(|| OfferingFqnError {
                        message: "Repo source requires format 'repo:namespace/offering'"
                            .to_string(),
                    })?;
                let repo = repo.trim().to_string();
                let offering = validate_fqn_segment(offering_name, "offering")?;
                Ok(Self {
                    source: Some(OfferingSource::Repo(repo)),
                    offering,
                    instance,
                    image_ref: None,
                })
            }
            "oci:" => {
                let offering = validate_fqn_segment(name_part, "offering")?;
                Ok(Self {
                    source: Some(OfferingSource::Oci),
                    offering,
                    instance,
                    image_ref: None,
                })
            }
            _ => Err(OfferingFqnError {
                message: format!("Unknown source scheme: {}", scheme),
            }),
        }
    }
}

// ── Deprecated Wrapper ─────────────────────────────────────────────────────

/// Parse and validate an offering FQN string.
///
/// Deprecated: use `OfferingFqn::parse()` instead.
#[deprecated(note = "Use OfferingFqn::parse() instead")]
pub fn parse_offering_fqn(input: &str) -> Result<OfferingFqn, OfferingFqnError> {
    OfferingFqn::parse(input)
}

// ── Validation Helpers ─────────────────────────────────────────────────────

/// Validate and normalize an FQN segment (offering name or instance name).
///
/// Rules:
/// - Lowercase, trimmed
/// - Allowed characters: `[a-z0-9_-]`
/// - Must start with a letter
/// - Cannot contain `--` (reserved for container encoding)
/// - Max 128 characters
fn validate_fqn_segment(segment: &str, label: &str) -> Result<String, OfferingFqnError> {
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

/// Extract a short offering name from a Docker image reference.
///
/// `"nginx:latest"` → `"nginx"`, `"ghcr.io/org/app:v2"` → `"app"`.
fn offering_name_from_image_ref(image_ref: &str) -> String {
    // Strip tag: everything after last ':'
    let without_tag = image_ref
        .rsplit_once(':')
        .map(|(left, _)| left)
        .unwrap_or(image_ref);
    // Take the last path segment: "ghcr.io/org/app" → "app"
    let name = without_tag
        .rsplit_once('/')
        .map(|(_, right)| right)
        .unwrap_or(without_tag);
    name.to_lowercase()
}

/// Sanitize a Docker image reference for use in container names.
///
/// Replaces `/` and `:` with `-`, preserves dots where valid.
fn sanitize_image_ref_for_container(image_ref: &str) -> String {
    image_ref
        .chars()
        .map(|c| match c {
            '/' | ':' => '-',
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => c,
            _ => '-',
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod fqn_tests {
    use super::*;

    // ── V2 Parsing ─────────────────────────────────────────────────────

    #[test]
    fn parse_default_instance() {
        let fqn = OfferingFqn::parse("mongodb").unwrap();
        assert_eq!(fqn.offering, "mongodb");
        assert!(fqn.instance.is_none());
        assert!(fqn.source.is_none());
        assert_eq!(fqn.fqn(), "mongodb");
    }

    #[test]
    fn parse_named_instance() {
        let fqn = OfferingFqn::parse("mongodb::prod").unwrap();
        assert_eq!(fqn.offering, "mongodb");
        assert_eq!(fqn.instance.as_deref(), Some("prod"));
        assert!(fqn.source.is_none());
        assert_eq!(fqn.fqn(), "mongodb::prod");
    }

    #[test]
    fn parse_canonicalizes_self_instance() {
        let fqn = OfferingFqn::parse("mongodb::mongodb").unwrap();
        assert_eq!(fqn.offering, "mongodb");
        assert!(fqn.instance.is_none());
        assert_eq!(fqn.fqn(), "mongodb");
    }

    #[test]
    fn parse_normalizes_case() {
        let fqn = OfferingFqn::parse("Ollama::DEV").unwrap();
        assert_eq!(fqn.fqn(), "ollama::dev");
    }

    // ── Legacy Normalization ───────────────────────────────────────────

    #[test]
    fn parse_v1_legacy_single_colon() {
        let fqn = OfferingFqn::parse("ollama:dev").unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("dev"));
        assert_eq!(fqn.fqn(), "ollama::dev");
    }

    #[test]
    fn parse_v1_legacy_adopted() {
        let fqn = OfferingFqn::parse("ollama:adopted").unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("adopted"));
        assert_eq!(fqn.fqn(), "ollama::adopted");
    }

    #[test]
    fn parse_v0_legacy_at_sign() {
        let fqn = OfferingFqn::parse("ollama@adopted").unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("adopted"));
        assert_eq!(fqn.fqn(), "ollama::adopted");
    }

    // ── Source Schemes ─────────────────────────────────────────────────

    #[test]
    fn parse_image_direct_simple() {
        let fqn = OfferingFqn::parse("image:nginx:latest").unwrap();
        assert_eq!(fqn.source, Some(OfferingSource::Image));
        assert_eq!(fqn.offering, "nginx");
        assert_eq!(fqn.image_ref.as_deref(), Some("nginx:latest"));
        assert!(fqn.instance.is_none());
        assert_eq!(fqn.fqn(), "image:nginx:latest");
    }

    #[test]
    fn parse_image_direct_with_instance() {
        let fqn = OfferingFqn::parse("image:nginx:latest::staging").unwrap();
        assert_eq!(fqn.source, Some(OfferingSource::Image));
        assert_eq!(fqn.offering, "nginx");
        assert_eq!(fqn.image_ref.as_deref(), Some("nginx:latest"));
        assert_eq!(fqn.instance.as_deref(), Some("staging"));
        assert_eq!(fqn.fqn(), "image:nginx:latest::staging");
    }

    #[test]
    fn parse_image_direct_ghcr() {
        let fqn = OfferingFqn::parse("image:ghcr.io/org/app:v2").unwrap();
        assert_eq!(fqn.source, Some(OfferingSource::Image));
        assert_eq!(fqn.offering, "app");
        assert_eq!(fqn.image_ref.as_deref(), Some("ghcr.io/org/app:v2"));
    }

    #[test]
    fn parse_image_direct_ghcr_with_instance() {
        let fqn = OfferingFqn::parse("image:ghcr.io/org/app:v2::prod").unwrap();
        assert_eq!(fqn.source, Some(OfferingSource::Image));
        assert_eq!(fqn.offering, "app");
        assert_eq!(fqn.instance.as_deref(), Some("prod"));
        assert_eq!(fqn.fqn(), "image:ghcr.io/org/app:v2::prod");
    }

    #[test]
    fn parse_image_direct_tag_only() {
        let fqn = OfferingFqn::parse("image:mongo:7").unwrap();
        assert_eq!(fqn.source, Some(OfferingSource::Image));
        assert_eq!(fqn.offering, "mongo");
        assert_eq!(fqn.image_ref.as_deref(), Some("mongo:7"));
    }

    #[test]
    fn parse_repo_source() {
        let fqn = OfferingFqn::parse("repo:community/bookstack").unwrap();
        assert_eq!(
            fqn.source,
            Some(OfferingSource::Repo("community".to_string()))
        );
        assert_eq!(fqn.offering, "bookstack");
        assert!(fqn.instance.is_none());
        assert_eq!(fqn.fqn(), "repo:community/bookstack");
    }

    #[test]
    fn parse_repo_source_with_instance() {
        let fqn = OfferingFqn::parse("repo:community/bookstack::dev").unwrap();
        assert_eq!(
            fqn.source,
            Some(OfferingSource::Repo("community".to_string()))
        );
        assert_eq!(fqn.offering, "bookstack");
        assert_eq!(fqn.instance.as_deref(), Some("dev"));
        assert_eq!(fqn.fqn(), "repo:community/bookstack::dev");
    }

    // ── Builders ───────────────────────────────────────────────────────

    #[test]
    fn builder_new() {
        let fqn = OfferingFqn::new("mongodb").unwrap();
        assert_eq!(fqn.fqn(), "mongodb");
    }

    #[test]
    fn builder_with_instance() {
        let fqn = OfferingFqn::with_instance("mongodb", "prod").unwrap();
        assert_eq!(fqn.fqn(), "mongodb::prod");
    }

    #[test]
    fn builder_adopted() {
        let fqn = OfferingFqn::adopted("ollama").unwrap();
        assert_eq!(fqn.fqn(), "ollama::adopted");
    }

    #[test]
    fn builder_image_direct() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        assert_eq!(fqn.fqn(), "image:nginx:latest");
        assert!(fqn.is_image_direct());
        assert!(!fqn.is_curated());
    }

    #[test]
    fn builder_image_direct_with_instance() {
        let fqn = OfferingFqn::image_direct_with_instance("nginx:latest", "staging").unwrap();
        assert_eq!(fqn.fqn(), "image:nginx:latest::staging");
    }

    // ── Container Encoding ─────────────────────────────────────────────

    #[test]
    fn container_encoding_curated_default() {
        let fqn = OfferingFqn::parse("mongodb").unwrap();
        assert_eq!(fqn.encoded_for_container(), "mongodb");
    }

    #[test]
    fn container_encoding_curated_instance() {
        let fqn = OfferingFqn::parse("mongodb::prod").unwrap();
        assert_eq!(fqn.encoded_for_container(), "mongodb--prod");
    }

    #[test]
    fn container_encoding_v1_legacy_matches_v2() {
        // V1 and V2 produce identical container names
        let v1 = OfferingFqn::parse("mongodb:prod").unwrap();
        let v2 = OfferingFqn::parse("mongodb::prod").unwrap();
        assert_eq!(v1.encoded_for_container(), v2.encoded_for_container());
        assert_eq!(v1.encoded_for_container(), "mongodb--prod");
    }

    #[test]
    fn container_encoding_image_direct() {
        let fqn = OfferingFqn::parse("image:nginx:latest").unwrap();
        assert_eq!(fqn.encoded_for_container(), "img-nginx-latest");
    }

    #[test]
    fn container_encoding_image_direct_with_instance() {
        let fqn = OfferingFqn::parse("image:nginx:latest::staging").unwrap();
        assert_eq!(fqn.encoded_for_container(), "img-nginx-latest--staging");
    }

    #[test]
    fn container_encoding_image_direct_ghcr() {
        let fqn = OfferingFqn::parse("image:ghcr.io/org/app:v2").unwrap();
        assert_eq!(fqn.encoded_for_container(), "img-ghcr.io-org-app-v2");
    }

    // ── Display ────────────────────────────────────────────────────────

    #[test]
    fn display_matches_fqn() {
        let fqn = OfferingFqn::parse("mongodb::prod").unwrap();
        assert_eq!(format!("{}", fqn), "mongodb::prod");
        assert_eq!(fqn.to_string(), fqn.fqn());
    }

    // ── Serde Round-Trip ───────────────────────────────────────────────

    #[test]
    fn serde_roundtrip_curated() {
        let fqn = OfferingFqn::parse("mongodb::prod").unwrap();
        let json = serde_json::to_string(&fqn).unwrap();
        assert_eq!(json, "\"mongodb::prod\"");
        let parsed: OfferingFqn = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fqn);
    }

    #[test]
    fn serde_roundtrip_image_direct() {
        let fqn = OfferingFqn::parse("image:nginx:latest::staging").unwrap();
        let json = serde_json::to_string(&fqn).unwrap();
        assert_eq!(json, "\"image:nginx:latest::staging\"");
        let parsed: OfferingFqn = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fqn);
    }

    #[test]
    fn serde_deserialize_v1_legacy() {
        let json = "\"ollama:adopted\"";
        let fqn: OfferingFqn = serde_json::from_str(json).unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("adopted"));
        // Re-serializes as V2
        assert_eq!(serde_json::to_string(&fqn).unwrap(), "\"ollama::adopted\"");
    }

    #[test]
    fn serde_deserialize_v0_legacy() {
        let json = "\"ollama@adopted\"";
        let fqn: OfferingFqn = serde_json::from_str(json).unwrap();
        assert_eq!(fqn.offering, "ollama");
        assert_eq!(fqn.instance.as_deref(), Some("adopted"));
        assert_eq!(serde_json::to_string(&fqn).unwrap(), "\"ollama::adopted\"");
    }

    // ── Hash ───────────────────────────────────────────────────────────

    #[test]
    fn hash_equal_values() {
        use std::collections::HashSet;
        let a = OfferingFqn::parse("mongodb::prod").unwrap();
        let b = OfferingFqn::parse("mongodb::prod").unwrap();
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn hash_as_map_key() {
        use std::collections::HashMap;
        let fqn = OfferingFqn::parse("ollama::adopted").unwrap();
        let mut map = HashMap::new();
        map.insert(fqn.clone(), 42);
        assert_eq!(map.get(&fqn), Some(&42));
    }

    // ── Rejection ──────────────────────────────────────────────────────

    #[test]
    fn reject_empty() {
        assert!(OfferingFqn::parse("").is_err());
    }

    #[test]
    fn reject_multiple_double_colons() {
        assert!(OfferingFqn::parse("a::b::c").is_err());
    }

    #[test]
    fn reject_invalid_chars() {
        assert!(OfferingFqn::parse("olla$ma").is_err());
    }

    #[test]
    fn reject_reserved_separator() {
        assert!(OfferingFqn::parse("olla--ma").is_err());
    }

    #[test]
    fn reject_starts_with_number() {
        assert!(OfferingFqn::parse("123abc").is_err());
    }

    #[test]
    fn reject_empty_image_ref() {
        assert!(OfferingFqn::parse("image:").is_err());
    }

    #[test]
    fn reject_repo_without_slash() {
        assert!(OfferingFqn::parse("repo:bookstack").is_err());
    }

    // ── Helpers ────────────────────────────────────────────────────────

    #[test]
    fn offering_name_extraction_from_image() {
        assert_eq!(offering_name_from_image_ref("nginx:latest"), "nginx");
        assert_eq!(offering_name_from_image_ref("mongo:7"), "mongo");
        assert_eq!(offering_name_from_image_ref("ghcr.io/org/app:v2"), "app");
        assert_eq!(offering_name_from_image_ref("nginx"), "nginx");
    }

    #[test]
    fn image_ref_sanitization() {
        assert_eq!(
            sanitize_image_ref_for_container("nginx:latest"),
            "nginx-latest"
        );
        assert_eq!(
            sanitize_image_ref_for_container("ghcr.io/org/app:v2"),
            "ghcr.io-org-app-v2"
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    // Regex for valid offering/instance segments: lowercase start,
    // alphanumeric with single hyphens (no --, no trailing -).
    const VALID_SEGMENT: &str = "[a-z]([a-z0-9](-[a-z0-9])?){0,10}";

    proptest! {
        /// OfferingFqn::parse must never panic on arbitrary input.
        /// It may return Ok or Err, but panicking is a bug.
        #[test]
        fn parse_never_panics(s in "\\PC{0,100}") {
            let _ = OfferingFqn::parse(&s);
        }

        /// Roundtrip: construct a valid FQN string, parse it, verify the
        /// offering name is preserved. Instance is only preserved when it
        /// differs from the offering name (canonicalization strips self-instances).
        #[test]
        fn roundtrip_fqn(
            offering in VALID_SEGMENT,
            instance in VALID_SEGMENT,
        ) {
            let fqn_str = format!("{offering}::{instance}");
            let parsed = OfferingFqn::parse(&fqn_str).unwrap();
            prop_assert_eq!(&parsed.offering, &offering);
            // Instance is canonicalized: "mongodb::mongodb" -> instance=None
            if offering == instance {
                prop_assert!(parsed.instance.is_none());
            } else {
                prop_assert_eq!(parsed.instance.as_deref(), Some(instance.as_str()));
            }
        }

        /// Display -> parse roundtrip: fqn() output must re-parse to an equal FQN.
        #[test]
        fn display_roundtrip(offering in VALID_SEGMENT) {
            let fqn = OfferingFqn::parse(&offering).unwrap();
            let displayed = fqn.fqn();
            let reparsed = OfferingFqn::parse(&displayed).unwrap();
            prop_assert_eq!(fqn.offering, reparsed.offering);
            prop_assert_eq!(fqn.instance, reparsed.instance);
        }

        /// encoded_for_container must never contain characters forbidden in
        /// Docker container names (only [a-zA-Z0-9_.-] are allowed).
        #[test]
        fn encoded_for_container_is_docker_safe(offering in VALID_SEGMENT) {
            let fqn = OfferingFqn::parse(&offering).unwrap();
            let encoded = fqn.encoded_for_container();
            for ch in encoded.chars() {
                prop_assert!(
                    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-',
                    "illegal char '{}' in encoded container name: {}",
                    ch,
                    encoded
                );
            }
        }
    }
}
