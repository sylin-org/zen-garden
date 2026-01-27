//! Adapter types for Zen Garden
//!
//! NOTE: This module is being consolidated. Prefer using:
//! - `garden_common::command_manifest::CommandManifest` for adapter manifests
//! - `garden_common::command_manifest::CommandResponse` for responses
//! - `garden_common::command_manifest::AdapterCommandRequest` for requests
//!
//! This module will be deprecated in a future version.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export from command_manifest for backwards compatibility
pub use crate::command_manifest::{
    CommandResponse as AdapterCommandResponse,
    AdapterCommandRequest,
    CommandManifest as AdapterManifest,
    CommandDef as AdapterCommand,
};

/// Legacy adapter command request (deprecated - use AdapterCommandRequest)
#[deprecated(since = "0.2.0", note = "Use command_manifest::AdapterCommandRequest")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyAdapterCommandRequest {
    /// Adapter ID (e.g., "cricket", "lantern", future adapters)
    pub adapter_id: String,
    
    /// Command name (e.g., "play", "stop", "set_tune", "set_volume")
    pub command: String,
    
    /// Optional command parameters
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, String>,
}

/// Adapter registry response (list of available adapters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRegistryResponse {
    pub adapters: Vec<AdapterSummary>,
}

/// Summary of an adapter for registry listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSummary {
    /// Adapter ID
    pub adapter: String,
    
    /// Adapter type (presence, display, hardware)
    #[serde(rename = "type")]
    pub adapter_type: String,
    
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
    fn test_adapter_registry_response() {
        let resp = AdapterRegistryResponse {
            adapters: vec![
                AdapterSummary {
                    adapter: "cricket".to_string(),
                    adapter_type: "presence".to_string(),
                    version: "0.1.0".to_string(),
                    description: "Audio adapter".to_string(),
                    enabled: true,
                    running: true,
                },
            ],
        };
        
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("cricket"));
        assert!(json.contains("presence"));
    }
}
