//! Nourishment types - Shared models for update management
//!
//! These types are used by both moss (API) and rake (CLI) to ensure
//! consistent handling of software and firmware updates.

use serde::{Deserialize, Serialize};

/// Firmware confidence level - indicates how much we've validated this update
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum FirmwareConfidence {
    /// Matched against a hardware manifest - we've tested this device/version
    Tested,
    /// From LVFS/fwupd but not in our manifests - cryptographically signed but not garden-tested
    #[default]
    Suggested,
}

/// Unified update model - discriminated by type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Update {
    #[serde(rename = "offering")]
    Offering {
        name: String,
        current: String,
        available: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        age_days: Option<u32>,
    },
    #[serde(rename = "firmware")]
    Firmware {
        device_id: String,
        name: String,
        vendor: String,
        current: String,
        available: String,
        requires_reboot: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Confidence level - Tested (from manifest) or Suggested (from LVFS)
        #[serde(default)]
        confidence: FirmwareConfidence,
    },
    /// Moss daemon self-update (from GitHub Releases)
    #[serde(rename = "moss")]
    Moss {
        current: String,
        available: String,
        /// Download URL for the platform-matching package asset
        #[serde(skip_serializing_if = "Option::is_none")]
        download_url: Option<String>,
    },
}

/// Updates collection with available and blocked items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Updates {
    pub available: Vec<Update>,
    pub blocked: Vec<BlockedUpdate>,
}

/// Update blocked by constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedUpdate {
    #[serde(flatten)]
    pub update: Update,
    pub reason: String,
}

/// Local check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NourishmentCheckResponse {
    pub stone_name: String,
    pub updates: Updates,
}

/// Garden-wide check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenNourishmentResponse {
    pub stones: Vec<NourishmentCheckResponse>,
}

/// Execute request - scope-based
///
/// Rake sends intent only. Each stone interprets and applies its pending updates.
/// Examples:
///   {"scope": "all"}           - Apply all available updates
///   {"scope": "offerings"}     - Apply offering updates only  
///   {"scope": "firmware"}      - Apply firmware updates only
///
/// Future V1+: items field for granular selection
///   {"items": ["offering:ollama", "firmware:abc123"]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    /// Scope of updates to apply (default: all)
    #[serde(default)]
    pub scope: UpdateScope,
    /// Specific items to update (overrides scope if present)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
}

impl Default for ExecuteRequest {
    fn default() -> Self {
        Self {
            scope: UpdateScope::All,
            items: Vec::new(),
        }
    }
}

/// What scope of updates to apply
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateScope {
    /// All available updates (offerings + firmware + moss)
    #[default]
    All,
    /// Only offering (software) updates
    Offerings,
    /// Only firmware updates
    Firmware,
    /// Only Moss daemon self-updates
    Moss,
}

/// Execute response with job ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub job_id: String,
}

/// Garden-wide execute response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenExecuteResponse {
    pub job_id: String,
    pub stone_jobs: Vec<StoneJobStatus>,
}

/// Status of a stone's update job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneJobStatus {
    pub stone_name: String,
    pub state: StoneJobState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// State of a stone job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoneJobState {
    Pending,
    Running,
    Success,
    Failed,
    Unreachable,
}
