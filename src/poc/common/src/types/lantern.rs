//! Lantern service registry types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub endpoint: String,
    pub services: Vec<RegisterServiceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterServiceInfo {
    pub name: String,
    pub service_type: String,
    pub status: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub ttl_seconds: u32,
    pub next_heartbeat_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub service_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveResponse {
    pub stone_name: String,
    pub endpoint: String,
    pub service: ResolveServiceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveServiceInfo {
    pub name: String,
    pub service_type: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternTopology {
    pub stones: Vec<LanternStoneState>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternStoneState {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub name: String,
    pub endpoint: String,
    pub status: String,
    pub services: Vec<LanternServiceState>,
    pub last_seen: String,
    pub first_seen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline_since: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanternServiceState {
    pub name: String,
    pub service_type: String,
    pub status: String,
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenEvent {
    pub event_type: String,
    pub timestamp: String,
    pub stone_name: String,
    pub details: serde_json::Value,
}
