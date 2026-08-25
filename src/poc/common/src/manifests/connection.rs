//! Connection profile definitions for offerings and categories.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Structured connection profile for an offering or category.
///
/// - `protocol` is the primary scheme hint (e.g., "http", "postgresql").
/// - `uri_template` defines how to build the base connection URI.
/// - `endpoints` are named paths relative to the base URI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_template: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, String>,
}
