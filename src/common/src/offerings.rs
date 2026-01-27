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
