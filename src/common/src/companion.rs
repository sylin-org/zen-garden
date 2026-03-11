//! Companion value objects and legacy shims.
//!
//! ## Canonical types (ARCH-0003)
//!
//! - [`Companion`] — the authoritative companion description.
//! - [`Manifest`]  — the companion's command manifest, nested inside [`Companion`].
//!
//! These replace scattered `companion_id: String`, `companion_name: String`
//! fields and the `CompanionManifest` alias. Consumers migrate wave-by-wave;
//! the legacy re-exports below will be removed once all callers are updated.

use crate::command_manifest::CommandDef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Canonical value objects (ARCH-0003 Wave 1b)
// ============================================================================

/// The authoritative description of a companion process.
///
/// Replaces scattered `companion_id: String` / `companion_name: String` fields
/// and the `CompanionManifest` alias. The manifest is a nested field — it is
/// not a peer type.
///
/// Access pattern:
/// ```text
/// companion.id                      // identifier (e.g. "cricket")
/// companion.name                    // display name (e.g. "Cricket")
/// companion.port                    // assigned port from ledger
/// companion.manifest.version        // manifest version
/// companion.manifest.commands       // available commands
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Companion {
    /// Companion identifier (e.g., "cricket", "firefly"). Permanent.
    pub id: String,

    /// Human-readable display name (e.g., "Cricket"). Changeable.
    pub name: String,

    /// Assigned port from the companion port ledger (7187–7199).
    pub port: u16,

    /// Command manifest — nested, not a peer type.
    pub manifest: Manifest,
}

/// A companion's command manifest — nested inside [`Companion`].
///
/// Contains metadata and the list of available commands. Mirrors the wire
/// format produced by `--dump-commands` but is the canonical in-memory form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Companion version string (e.g., "0.1.0").
    pub version: String,

    /// Short description of this companion.
    pub description: String,

    /// Available commands defined by this companion.
    #[serde(default)]
    pub commands: Vec<CommandDef>,
}

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
