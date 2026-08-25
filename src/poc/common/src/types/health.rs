//! Health types — health checks and daemon health status.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::constants::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceHealthStatus {
    Healthy,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: String, // "pass", "warn", or "fail"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String, // "healthy", "degraded", or "unhealthy"
    #[serde(flatten)]
    pub details: HashMap<String, serde_json::Value>,
}

impl ComponentHealth {
    pub fn healthy(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_HEALTHY.to_string(),
            details,
        }
    }

    pub fn degraded(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_DEGRADED.to_string(),
            details,
        }
    }

    pub fn unhealthy(details: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: HEALTH_UNHEALTHY.to_string(),
            details,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealthStatus {
    pub status: String,    // "healthy", "degraded", or "unhealthy"
    pub version: String,   // Software version (e.g., "0.1.202601231053")
    pub timestamp: String, // ISO 8601 timestamp
    pub components: HashMap<String, ComponentHealth>,
    // Platform information for deployment tools
    pub os: String,           // Operating system (e.g., "windows", "linux", "macos")
    pub architecture: String, // CPU architecture (e.g., "x86_64", "aarch64")
    /// Pond name when this stone is enrolled in a pond (absent = no pond)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub pond: Option<String>,
    // Legacy fields for backward compatibility
    #[serde(skip_serializing)]
    pub docker_available: bool,
    #[serde(skip_serializing)]
    pub disk_space_ok: bool,
    #[serde(skip_serializing)]
    pub memory_ok: bool,
    #[serde(skip_serializing)]
    pub uptime_seconds: u64,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub checks: HashMap<String, HealthCheck>,
}
