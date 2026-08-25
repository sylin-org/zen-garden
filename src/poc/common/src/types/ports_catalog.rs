//! Well-known ports catalog types.

use serde::{Deserialize, Serialize};

/// Catalog of well-known ports with conflict detection and remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellKnownPortsCatalog {
    pub version: String,
    pub ports: std::collections::HashMap<u16, WellKnownPort>,
}

/// Definition of a well-known port with platform-specific conflict handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellKnownPort {
    pub name: String,
    pub description: String,
    /// Default remediation for all platforms (used if no platform-specific handler)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<PortRemediation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<PortConflictHandler>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<PortConflictHandler>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<PortConflictHandler>,
}

/// Platform-specific conflict detection and remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConflictHandler {
    /// Common service that uses this port
    pub common_culprit: String,
    /// Command to detect if the culprit is active (exit 0 = active)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection: Option<String>,
    /// Remediation strategy
    pub remediation: PortRemediation,
}

/// Remediation strategy for port conflicts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PortRemediation {
    /// Remap to next available port in range (default for most services)
    Remap {
        /// Start of port range to search
        range_start: u16,
        /// End of port range to search
        range_end: u16,
    },
    /// Automatically run commands to free the port (for essential ports like DNS)
    Auto {
        /// Commands to run to free the port
        commands: Vec<String>,
        /// Files to create after remediation
        #[serde(default, skip_serializing_if = "Option::is_none")]
        files: Option<Vec<RemediationFile>>,
    },
    /// Show message and fail - user must manually resolve
    Manual { message: String },
    /// Fail with error - no remediation possible
    Fail { message: String },
}

/// File to create as part of remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationFile {
    pub path: String,
    pub content: String,
}
