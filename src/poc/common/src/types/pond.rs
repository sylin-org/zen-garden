//! Pond security types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PondConfig {
    pub enabled: bool,
    pub keystone_path: Option<String>,
    pub require_mtls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystoneRequest {
    pub pond_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInviteRequest {
    pub stone_name: String,
    pub expiry_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInviteResponse {
    pub invitation_code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceStoneRequest {
    pub invitation_code: String,
}
