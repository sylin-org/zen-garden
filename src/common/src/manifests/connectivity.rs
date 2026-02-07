//! Connectivity enforcement configuration
//!
//! Defines OS-specific checks and ensure commands to make adopted services
//! reachable on the local network (e.g., bind to LAN interface, open firewall).

use serde::{Deserialize, Serialize};
use super::detection::DetectionRule;

/// Connectivity configuration for adopted offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityConfig {
    /// Whether to enforce connectivity automatically (default: true)
    #[serde(default = "default_enforce_true")]
    pub enforce: bool,

    /// Cooldown between enforcement attempts in seconds (default: 30)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub enforce_cooldown_secs: Option<u64>,

    /// Maximum enforcement attempts before pausing (default: 5)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_attempts: Option<u32>,

    /// Windows connectivity rules
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub windows: Option<ConnectivityRules>,

    /// Linux connectivity rules
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub linux: Option<ConnectivityRules>,

    /// macOS connectivity rules
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub macos: Option<ConnectivityRules>,
}

impl ConnectivityConfig {
    /// Get connectivity rules for the current OS
    pub fn get_current_os_rules(&self) -> Option<&ConnectivityRules> {
        #[cfg(target_os = "windows")]
        return self.windows.as_ref();

        #[cfg(target_os = "linux")]
        return self.linux.as_ref();

        #[cfg(target_os = "macos")]
        return self.macos.as_ref();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        None
    }

    /// Get enforcement cooldown in seconds (default: 30)
    pub fn enforce_cooldown_secs(&self) -> u64 {
        self.enforce_cooldown_secs.unwrap_or(30)
    }

    /// Get maximum enforcement attempts (default: 5, 0 = unlimited)
    pub fn max_attempts(&self) -> u32 {
        let value = self.max_attempts.unwrap_or(5);
        if value == 0 {
            u32::MAX
        } else {
            value
        }
    }
}

/// OS-specific connectivity checks and ensure actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityRules {
    /// Checks that determine whether connectivity is properly configured
    #[serde(default)]
    pub checks: Vec<DetectionRule>,

    /// Commands to run to enforce connectivity if checks fail
    #[serde(default)]
    pub ensure: Vec<CommandAction>,
}

/// Command action for connectivity enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAction {
    /// Shell command to execute
    pub command: String,

    /// Timeout in seconds (default: 30)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timeout_secs: Option<u64>,

    /// Whether to continue when this command fails (default: false)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub continue_on_error: Option<bool>,
}

fn default_enforce_true() -> bool {
    true
}
