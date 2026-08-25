//! Detection and Control Types
//!
//! Types for service detection, control, and health checking.
//! Used by the adopted mode to detect and manage native services.

use crate::types::{AdoptedControlLevel, HealthMethod};
use serde::{Deserialize, Serialize};

/// OS-specific detection rules (legacy command-based).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsDetectionRules {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub windows: Option<Vec<DetectionRule>>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub linux: Option<Vec<DetectionRule>>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub macos: Option<Vec<DetectionRule>>,
}

impl OsDetectionRules {
    /// Get detection rules for the current OS
    pub fn get_current_os_rules(&self) -> Vec<DetectionRule> {
        #[cfg(target_os = "windows")]
        return self.windows.clone().unwrap_or_default();

        #[cfg(target_os = "linux")]
        return self.linux.clone().unwrap_or_default();

        #[cfg(target_os = "macos")]
        return self.macos.clone().unwrap_or_default();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        Vec::new()
    }
}

/// Detection rule for adopted offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionRule {
    /// Detection method
    pub method: DetectionMethod,

    /// Method-specific configuration (nested under `config:` key in YAML)
    pub config: DetectionConfig,

    /// Stability threshold (consecutive successes required)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stability_threshold: Option<u8>,

    /// Cache TTL in seconds (0 = no cache)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_ttl_secs: Option<u64>,
}

/// Detection method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMethod {
    /// Execute command (e.g., "mongod --version")
    Command,
    /// Inspect Docker container
    ContainerInspect,
    /// HTTP probe
    HttpProbe,
}

/// Detection configuration (method-specific)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DetectionConfig {
    Command(CommandDetection),
    ContainerInspect(ContainerInspectDetection),
    HttpProbe(HttpProbeDetection),
}

/// Command-based detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDetection {
    /// Command to execute (e.g., "ollama --version", "mongod --version")
    pub command: String,

    /// Expected output pattern (regex)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_pattern: Option<String>,

    /// Expected exit code (default: 0)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_exit_code: Option<i32>,
}

/// Container inspection detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInspectDetection {
    /// Container name pattern (regex)
    pub container_pattern: String,

    /// Expected image pattern (optional)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub image_pattern: Option<String>,
}

/// HTTP probe detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProbeDetection {
    /// URL to probe
    pub url: String,

    /// Expected HTTP status code (default: 200)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_status: Option<u16>,

    /// Timeout in milliseconds (default: 2000)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timeout_ms: Option<u64>,
}

// ── Process-Based Detection (DETECT-0001) ──────────────────────

/// Process-based detection configuration.
///
/// Replaces command-based detection with native process matching.
/// Cross-platform: same definition works on Windows, Linux, macOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDetection {
    /// Executable name to match (case-insensitive substring).
    /// e.g., "python", "ollama", "whisper-server"
    pub executable: String,

    /// Platform-specific executable name override.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub windows_executable: Option<String>,

    /// Platform-specific executable name override.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub linux_executable: Option<String>,

    /// Command line must contain this substring (case-insensitive).
    /// e.g., "speech.py", "serve", "main.py"
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cmdline_contains: Option<String>,
}

/// Health verification for process-based detection.
///
/// HTTP probe on the discovered port to confirm service identity.
/// Required for generic executables (python), optional for unique ones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthVerification {
    /// HTTP path to probe (e.g., "/health", "/system_stats").
    pub path: String,

    /// Expected HTTP status code (default: 200).
    #[serde(default = "default_expected_status")]
    pub expected_status: u16,

    /// Response body must contain this string.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub response_contains: Option<String>,
}

fn default_expected_status() -> u16 {
    200
}

/// Port configuration for process-based detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDetectionConfig {
    /// Default port to try if TCP table lookup yields nothing.
    pub default: u16,

    /// Port range to scan as last resort [start, end].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub range: Option<(u16, u16)>,

    /// Persist discovered port across restarts.
    #[serde(default = "default_true")]
    pub remember: bool,
}

fn default_true() -> bool {
    true
}

/// Control configuration for adopted offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlConfig {
    /// Control level (default: monitor)
    #[serde(default)]
    pub level: AdoptedControlLevel,

    /// Start command (required for full control)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_command: Option<String>,

    /// Stop command (required for full control)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stop_command: Option<String>,

    /// Restart command (optional, defaults to stop + start)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restart_command: Option<String>,

    /// Health check URL for monitoring
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub health_check_url: Option<String>,
}

/// Location configuration for borrowed offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationConfig {
    pub host: String,
    pub port: u16,
    pub protocol: String,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Health check method (default: http)
    #[serde(default = "default_health_method")]
    pub method: HealthMethod,

    /// Interval in seconds (default: 30)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub interval_secs: Option<u64>,

    /// Timeout in milliseconds (default: 2000)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timeout_ms: Option<u64>,

    /// HTTP-specific: endpoint path
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub http_path: Option<String>,
}

fn default_health_method() -> HealthMethod {
    HealthMethod::Http
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_level_default() {
        let config = ControlConfig {
            level: AdoptedControlLevel::default(),
            start_command: None,
            stop_command: None,
            restart_command: None,
            health_check_url: None,
        };
        assert_eq!(config.level, AdoptedControlLevel::Monitor);
    }

    #[test]
    fn test_detection_rule_serialization() {
        let rule = DetectionRule {
            method: DetectionMethod::Command,
            config: DetectionConfig::Command(CommandDetection {
                command: "test --version".into(),
                expected_pattern: None,
                expected_exit_code: Some(0),
            }),
            stability_threshold: Some(2),
            cache_ttl_secs: None,
        };

        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("test --version"));
    }
}
