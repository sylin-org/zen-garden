//! Generic distributed election protocol for Zen Garden
//!
//! This module implements a lightweight, stateless election protocol that allows
//! any stone to request a "winner" from a set of candidates without centralized
//! coordination. See docs/specs/ELECTION-0001-distributed-election.md for full specification.
//!
//! # Key Features
//!
//! - **Stateless**: No persistent state beyond active timers
//! - **Requester-owned**: Requester controls the election flow
//! - **Deterministic**: BLAKE3 hash-based delays ensure reproducible ordering
//! - **Self-excluding**: Requesters automatically ignore own elections
//! - **Concurrent-safe**: Multiple elections via unique election_id
//! - **Service-agnostic**: Generic module for any election use case
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use garden_common::election::{Election, ElectionType};
//! use serde_json::json;
//!
//! // Start an election
//! let winner = Election::new(ElectionType::UpdateSource)
//!     .with_criteria(json!({
//!         "moss_version": {"$gt": "0.1.309"}
//!     }))
//!     .timeout(Duration::from_secs(10))
//!     .run()
//!     .await?;
//!
//! if let Some(winner) = winner {
//!     // Use winner stone_id to get endpoint from topology cache
//!     download_update(&winner.stone_id).await?;
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// Types
// ============================================================================

/// Election type identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ElectionType {
    /// Find stone with newer version to download update from
    UpdateSource,
    /// Find stone to coordinate a multi-stone ceremony
    CeremonyCoordinator,
    /// Find stone to receive offering replica
    ReplicaTarget,
    /// Find stone with stored backup to restore from
    BackupSource,
    /// Elect primary for a replicated offering (ORCH-0001).
    /// Carries the offering FQN (e.g. `"weaviate:dev"`) so every recipient
    /// knows the election scope without inspecting the criteria bag.
    /// Uses `ScoreMechanism::Fitness` — candidates respond with fitness scores.
    OfferingPrimary(String),
    /// Custom election type
    Custom(String),
}

/// How candidates are ranked during an election.
///
/// - **Blake** (default): BLAKE3 hash delay, first respondent wins. Suitable
///   for simple arbitrary-winner elections (update source, ceremony coordinator).
/// - **Fitness**: Candidates respond immediately with a fitness score (i16).
///   Highest score wins. `1001` = pinned (always wins). Ineligible stones
///   simply don't respond.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScoreMechanism {
    /// BLAKE3 hash → delay → first respondent wins (existing).
    #[default]
    Blake,
    /// Candidates respond immediately with a fitness score (i16).
    Fitness,
}

/// Election request message (broadcast to all candidates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionRequest {
    /// Unique identifier for this election (GUIDv7)
    pub election_id: String,
    /// Type of election
    pub election_type: ElectionType,
    /// BSON-style filter criteria
    pub criteria: Value,
    /// How candidates are ranked. Default: Blake.
    #[serde(default)]
    pub score_mechanism: ScoreMechanism,
}

/// Election candidate response (unicast to requester)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionCandidate {
    /// Must match request election_id
    pub election_id: String,
    /// Stone ID of candidate
    pub stone_id: String,
    /// Name of candidate (for logging)
    pub stone_name: String,
    /// Fitness score [-1000..1000], or 1001 if pinned.
    /// Present only in Fitness mode; absent in Blake mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<i16>,
    /// ISO 8601 timestamp of pin — tiebreaker when multiple candidates score 1001.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_timestamp: Option<String>,
}

/// Election result announcement (broadcast to abort other candidates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionResult {
    /// Must match request election_id
    pub election_id: String,
    /// Stone ID of winner
    pub winner_id: String,
}

/// Election winner returned to requester
#[derive(Debug, Clone)]
pub struct ElectionWinner {
    /// Stone ID of winner
    pub stone_id: String,
    /// Stone name of winner (for logging)
    pub stone_name: String,
}

// ============================================================================
// Delay Calculation
// ============================================================================

/// Calculate deterministic election delay using BLAKE3 hash
///
/// Based on the algorithm from lantern/src/election.rs, adapted for
/// generic stone election protocol.
///
/// # Algorithm
///
/// 1. Hash input: `election:{stone_id}:{election_id}`
/// 2. Take first byte (0-255)
/// 3. Multiply by 30ms
/// 4. Result: 0-7650ms range with 30ms granularity
///
/// # Arguments
///
/// * `stone_id` - Unique identifier of this stone
/// * `election_id` - Unique identifier of this election
///
/// # Returns
///
/// Delay duration (0-7650ms)
pub fn calculate_election_delay(stone_id: &str, election_id: &str) -> Duration {
    let input = format!("election:{}:{}", stone_id, election_id);
    let hash = blake3::hash(input.as_bytes());

    // First byte (0-255) × 30ms = 0-7650ms spread
    let delay_ms = (hash.as_bytes()[0] as u64) * 30;

    Duration::from_millis(delay_ms)
}

// ============================================================================
// Criteria Evaluation
// ============================================================================

/// Evaluate BSON-style criteria against provided state
///
/// All conditions must match (implicit $and). Unknown operators are ignored.
///
/// # Supported Operators
///
/// - `$eq`: Equals
/// - `$ne`: Not equals
/// - `$gt`: Greater than
/// - `$gte`: Greater than or equal
/// - `$lt`: Less than
/// - `$lte`: Less than or equal
/// - `$in`: Value in array
/// - `$nin`: Value not in array
/// - `$exists`: Field exists (or doesn't if false)
///
/// # Arguments
///
/// * `criteria` - JSON object with BSON-style operators
/// * `state` - HashMap of field names to values
///
/// # Returns
///
/// `true` if all conditions match, `false` otherwise
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// use std::collections::HashMap;
///
/// let criteria = json!({
///     "moss_version": {"$gt": "0.1.309"},
///     "health": {"$in": ["thriving", "recovering"]}
/// });
///
/// let mut state = HashMap::new();
/// state.insert("moss_version".to_string(), json!("0.1.310"));
/// state.insert("health".to_string(), json!("thriving"));
///
/// assert!(matches_criteria(&criteria, &state));
/// ```
pub fn matches_criteria(criteria: &Value, state: &HashMap<String, Value>) -> bool {
    let obj = match criteria.as_object() {
        Some(o) => o,
        None => return true, // No criteria = always match
    };

    for (field, condition) in obj {
        let my_value = state.get(field);
        if !evaluate_condition(condition, my_value) {
            return false;
        }
    }
    true
}

/// Evaluate a single condition against actual value
fn evaluate_condition(condition: &Value, actual: Option<&Value>) -> bool {
    let cond_obj = match condition.as_object() {
        Some(o) => o,
        None => {
            // Direct value comparison (e.g., {"field": "value"} means {"field": {"$eq": "value"}})
            return actual == Some(condition);
        }
    };

    for (op, expected) in cond_obj {
        let result = match op.as_str() {
            "$eq" => actual == Some(expected),
            "$ne" => actual != Some(expected),
            "$gt" => compare_values(actual, expected) == Some(Ordering::Greater),
            "$gte" => matches!(
                compare_values(actual, expected),
                Some(Ordering::Greater | Ordering::Equal)
            ),
            "$lt" => compare_values(actual, expected) == Some(Ordering::Less),
            "$lte" => matches!(
                compare_values(actual, expected),
                Some(Ordering::Less | Ordering::Equal)
            ),
            "$in" => expected
                .as_array()
                .map(|arr| actual.map(|v| arr.contains(v)).unwrap_or(false))
                .unwrap_or(false),
            "$nin" => expected
                .as_array()
                .map(|arr| actual.map(|v| !arr.contains(v)).unwrap_or(true))
                .unwrap_or(true),
            "$exists" => {
                let should_exist = expected.as_bool().unwrap_or(true);
                actual.is_some() == should_exist
            }
            _ => {
                tracing::debug!(operator = %op, "Unknown criteria operator, ignoring");
                true // Unknown operator = skip
            }
        };

        if !result {
            return false;
        }
    }
    true
}

/// Compare two JSON values with semver-aware string comparison
fn compare_values(a: Option<&Value>, b: &Value) -> Option<Ordering> {
    let a = a?;

    // Try numeric comparison first
    if let (Some(a_num), Some(b_num)) = (a.as_f64(), b.as_f64()) {
        return a_num.partial_cmp(&b_num);
    }

    // Try string comparison (with semver awareness)
    if let (Some(a_str), Some(b_str)) = (a.as_str(), b.as_str()) {
        // Check if both look like versions (contains digits and dots)
        if is_version_like(a_str) && is_version_like(b_str) {
            return compare_versions(a_str, b_str);
        }
        return Some(a_str.cmp(b_str));
    }

    // Fallback: try to compare as strings
    let a_str = a.to_string();
    let b_str = b.to_string();
    Some(a_str.cmp(&b_str))
}

/// Check if string looks like a version number
fn is_version_like(s: &str) -> bool {
    s.contains('.') && s.chars().any(|c| c.is_ascii_digit())
}

/// Compare version strings with semver awareness
///
/// Splits on '.' and compares numeric segments first, then alphanumeric
fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        // Try numeric comparison first
        if let (Ok(a_num), Ok(b_num)) = (a_part.parse::<u64>(), b_part.parse::<u64>()) {
            match a_num.cmp(&b_num) {
                Ordering::Equal => continue,
                other => return Some(other),
            }
        }

        // Fall back to string comparison
        match a_part.cmp(b_part) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }

    // If all compared parts are equal, longer version is greater
    Some(a_parts.len().cmp(&b_parts.len()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_calculate_delay_deterministic() {
        let stone_id = "019bece4-42e5-7abc-1234567890ab";
        let election_id = "019bf5a2-1234-7abc-abcdef123456";

        let delay1 = calculate_election_delay(stone_id, election_id);
        let delay2 = calculate_election_delay(stone_id, election_id);

        assert_eq!(delay1, delay2, "Same inputs should produce same delay");
    }

    #[test]
    fn test_calculate_delay_range() {
        // Test that delays are within expected range (0-7650ms)
        for i in 0..100 {
            let stone_id = format!("stone-{}", i);
            let election_id = format!("election-{}", i);
            let delay = calculate_election_delay(&stone_id, &election_id);

            assert!(
                delay <= Duration::from_millis(7650),
                "Delay {} exceeds max",
                delay.as_millis()
            );
        }
    }

    #[test]
    fn test_criteria_eq() {
        let criteria = json!({"health": {"$eq": "thriving"}});
        let mut state = HashMap::new();
        state.insert("health".to_string(), json!("thriving"));

        assert!(matches_criteria(&criteria, &state));

        state.insert("health".to_string(), json!("recovering"));
        assert!(!matches_criteria(&criteria, &state));
    }

    #[test]
    fn test_criteria_gt_version() {
        let criteria = json!({"moss_version": {"$gt": "0.1.309"}});
        let mut state = HashMap::new();

        state.insert("moss_version".to_string(), json!("0.1.310"));
        assert!(matches_criteria(&criteria, &state));

        state.insert("moss_version".to_string(), json!("0.1.309"));
        assert!(!matches_criteria(&criteria, &state));

        state.insert("moss_version".to_string(), json!("0.1.308"));
        assert!(!matches_criteria(&criteria, &state));
    }

    #[test]
    fn test_criteria_in() {
        let criteria = json!({"health": {"$in": ["thriving", "recovering"]}});
        let mut state = HashMap::new();

        state.insert("health".to_string(), json!("thriving"));
        assert!(matches_criteria(&criteria, &state));

        state.insert("health".to_string(), json!("recovering"));
        assert!(matches_criteria(&criteria, &state));

        state.insert("health".to_string(), json!("dormant"));
        assert!(!matches_criteria(&criteria, &state));
    }

    #[test]
    fn test_criteria_exists() {
        let criteria = json!({"gpu": {"$exists": true}});
        let mut state = HashMap::new();

        state.insert("gpu".to_string(), json!("nvidia"));
        assert!(matches_criteria(&criteria, &state));

        state.remove("gpu");
        assert!(!matches_criteria(&criteria, &state));
    }

    #[test]
    fn test_criteria_multiple() {
        let criteria = json!({
            "moss_version": {"$gt": "0.1.309"},
            "health": {"$in": ["thriving", "recovering"]},
            "stone_id": {"$ne": "019becd8-exclude-this"}
        });

        let mut state = HashMap::new();
        state.insert("moss_version".to_string(), json!("0.1.310"));
        state.insert("health".to_string(), json!("thriving"));
        state.insert("stone_id".to_string(), json!("019bece4-valid-stone"));

        assert!(matches_criteria(&criteria, &state));

        // Fail one condition
        state.insert("stone_id".to_string(), json!("019becd8-exclude-this"));
        assert!(!matches_criteria(&criteria, &state));
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("0.1.310", "0.1.309"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_versions("0.2.0", "0.1.999"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_versions("1.0.0", "0.99.99"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_versions("0.1.309", "0.1.309"),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn test_empty_criteria() {
        let criteria = json!({});
        let state = HashMap::new();

        assert!(
            matches_criteria(&criteria, &state),
            "Empty criteria should match"
        );
    }
}
