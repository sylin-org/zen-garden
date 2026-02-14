//! Companion types for Zen Garden
//!
//! NOTE: This module is being consolidated. Prefer using:
//! - `garden_common::command_manifest::CommandManifest` for Companion manifests
//! - `garden_common::command_manifest::CommandResponse` for responses
//! - `garden_common::command_manifest::CompanionCommandRequest` for requests
//!
//! This module will be deprecated in a future version.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export from command_manifest for backwards compatibility
pub use crate::command_manifest::{
    CommandDef as CompanionCommand, CommandManifest as CompanionManifest,
    CommandResponse as CompanionCommandResponse, CompanionCommandRequest,
};

/// Legacy Companion command request (deprecated - use CompanionCommandRequest)
#[deprecated(
    since = "0.2.0",
    note = "Use command_manifest::CompanionCommandRequest"
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyCompanionCommandRequest {
    /// Companion ID (e.g., "cricket", "lantern", future Companions)
    pub companion_id: String,

    /// Command name (e.g., "play", "stop", "set_tune", "set_volume")
    pub command: String,

    /// Optional command parameters
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, String>,
}

/// Companion registry response (list of available companions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionRegistryResponse {
    pub companions: Vec<CompanionSummary>,
}

/// Summary of an Companion for registry listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSummary {
    /// Companion ID
    pub companion: String,

    /// Companion type (presence, display, hardware)
    #[serde(rename = "type")]
    pub companion_type: String,

    /// Version
    pub version: String,

    /// Description
    pub description: String,

    /// Whether enabled
    pub enabled: bool,

    /// Whether currently running
    pub running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_companion_registry_response() {
        let resp = CompanionRegistryResponse {
            companions: vec![CompanionSummary {
                companion: "cricket".to_string(),
                companion_type: "presence".to_string(),
                version: "0.1.0".to_string(),
                description: "Audio Companion".to_string(),
                enabled: true,
                running: true,
            }],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("cricket"));
        assert!(json.contains("presence"));
    }
}
