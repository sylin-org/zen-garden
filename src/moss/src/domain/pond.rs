//! Pond domain surface — enrollment state and cornerstone identity
//!
//! Exposes two read-only properties and one event:
//! - `enrolled: bool` — true when this stone has valid pond certificates
//! - `cornerstone: Option<String>` — hostname of the CA stone (if known)
//! - `name: Option<String>` — decorative pond name (e.g. "pond-still-lotus")
//! - `OnEnrollmentChange` — event emitted whenever enrollment state changes
//!
//! A cornerstone is always enrolled (placing the keystone issues a self-cert).
//! Non-cornerstone stones become enrolled after a successful proxy join.
//!
//! Consumers (HTTPS listener, chirp signing, mDNS) subscribe to the EventBus
//! for `DomainEvent::Pond(PondEvent::EnrollmentChanged)` and react accordingly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

/// Shared pond enrollment state.
///
/// Lives on `AppState`. Handlers mutate it via `set_enrolled` /
/// `set_cornerstone`; background listeners read it.
#[derive(Clone)]
pub struct PondState {
    enrolled: Arc<AtomicBool>,
    cornerstone: Arc<RwLock<Option<String>>>,
    name: Arc<RwLock<Option<String>>>,
}

impl PondState {
    pub fn new() -> Self {
        Self {
            enrolled: Arc::new(AtomicBool::new(false)),
            cornerstone: Arc::new(RwLock::new(None)),
            name: Arc::new(RwLock::new(None)),
        }
    }

    // ── Properties ──────────────────────────────────────────────────

    /// Is this stone enrolled in a pond (has valid certs)?
    pub fn enrolled(&self) -> bool {
        self.enrolled.load(Ordering::Relaxed)
    }

    /// The cornerstone hostname (CA holder), if known.
    pub async fn cornerstone(&self) -> Option<String> {
        self.cornerstone.read().await.clone()
    }

    /// The decorative pond name (e.g. "pond-still-lotus").
    pub async fn name(&self) -> Option<String> {
        self.name.read().await.clone()
    }

    // ── Mutations (called by handlers, emit event via EventBus) ─────

    /// Mark this stone as enrolled and set the cornerstone identity.
    /// Returns `true` if the enrolled state actually changed.
    pub async fn set_enrolled(&self, cornerstone: Option<String>) -> bool {
        *self.cornerstone.write().await = cornerstone;
        // swap returns old value; changed if old was false
        !self.enrolled.swap(true, Ordering::Relaxed)
    }

    /// Mark this stone as unenrolled (pond drained / cert revoked).
    /// Returns `true` if the enrolled state actually changed.
    pub async fn set_unenrolled(&self) -> bool {
        *self.cornerstone.write().await = None;
        *self.name.write().await = None;
        // swap returns old value; changed if old was true
        self.enrolled.swap(false, Ordering::Relaxed)
    }

    /// Seed state from persisted cert files at boot (no event emitted).
    pub fn seed_enrolled(&self, enrolled: bool) {
        self.enrolled.store(enrolled, Ordering::Relaxed);
    }

    /// Set the pond name (generated on init, or changed via rename).
    pub async fn set_name(&self, name: String) {
        *self.name.write().await = Some(name);
    }

    /// Seed the pond name from persisted storage at boot.
    pub async fn seed_name(&self, name: Option<String>) {
        *self.name.write().await = name;
    }
}

impl Default for PondState {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Pond metadata persistence — small JSON file at {data_dir}/pond.json
// ═══════════════════════════════════════════════════════════════════════

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
