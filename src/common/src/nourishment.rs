//! Nourishment types - Shared models for update management
//!
//! These types are used by both moss (API) and rake (CLI) to ensure
//! consistent handling of software and firmware updates.

use serde::{Deserialize, Serialize};

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

/// Execute request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub updates: Vec<UpdateSelector>,
}

/// Update selector for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum UpdateSelector {
    #[serde(rename = "offering")]
    Offering { name: String },
    #[serde(rename = "firmware")]
    Firmware { device_id: String },
}

/// Execute response with job ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub job_id: String,
}
