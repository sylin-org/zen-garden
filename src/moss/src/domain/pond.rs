//! Pond metadata persistence — small JSON file at {data_dir}/pond.json
//!
//! `PondState` (enrollment state) was absorbed into the Security aggregate
//! in ARCH-0027 (Book IX). This module retains only the metadata value
//! object and its persistence helpers, used at bootstrap and by API handlers.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// On-disk pond metadata (decorative, user-changeable).
#[derive(Serialize, Deserialize, Default)]
pub struct PondMetadata {
    /// Friendly pond name (e.g. "pond-still-lotus")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Load pond metadata from disk, returning default if absent or corrupt.
pub fn load_pond_metadata() -> PondMetadata {
    let path = PathBuf::from(garden_common::constants::paths::pond_metadata_file());
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => PondMetadata::default(),
    }
}

/// Persist pond metadata to disk.
pub fn save_pond_metadata(metadata: &PondMetadata) -> std::io::Result<()> {
    let path = PathBuf::from(garden_common::constants::paths::pond_metadata_file());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(metadata).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}
