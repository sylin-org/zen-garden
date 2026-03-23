//! API error types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}
