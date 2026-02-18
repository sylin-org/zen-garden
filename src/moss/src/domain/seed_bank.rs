//! Seed bank lifecycle domain object (STORAGE-0007)
//!
//! `SeedBank` is the single source of truth for a local seed bank's identity,
//! role, pin state, and I/O store. It **composes** a [`StorageDevice`] (infra)
//! and a [`SeedBankStore`] (infra) — enforcing mount verification before any
//! write operation.
//!
//! ## Design
//!
//! - **Domain layer**: owns identity, role, pin. No filesystem I/O directly.
//! - **Composes infrastructure**: delegates mount health to `StorageDevice`,
//!   file I/O to `SeedBankStore`.
//! - **Single lock**: `AppState::seed_banks` is the ONE map holding all local
//!   seed bank state. No more scattered `seed_bank_roles`, `seed_bank_pins`,
//!   `seed_bank_cache`, `mount_tracker`.

use garden_common::storage::{SeedBankInfo, SeedBankManifest, SeedBankRole};
use tracing::{debug, info, warn};

use crate::infra::storage::{SeedBankStore, StorageDevice};

// ============================================================================
// Pin state
// ============================================================================

/// Persisted pin state for a seed bank (Primary claim with GUIDv7 ordering).
#[derive(Debug, Clone)]
pub struct PinState {
    /// GUIDv7 pin identifier — higher value wins in last-pin-wins conflict.
    pub pin_id: String,
}

// ============================================================================
// SeedBank
// ============================================================================

/// Lifecycle object for a locally-mounted seed bank.
///
/// Created from a `StorageDevice` + `SeedBankManifest` at detection time.
/// Lives in `AppState::seed_banks` for the device's lifetime.
#[derive(Debug, Clone)]
pub struct SeedBank {
    // --- Identity (immutable after construction) ---
    /// Unique GUIDv7 per physical device. Primary key.
    pub id: String,
    /// First 8 hex chars of `id` — directory name.
    pub short_id: String,
    /// Logical seed bank name (FQN). Shared across replicas.
    pub name: String,
    /// Whether content is encrypted.
    pub encrypted: bool,
    /// Whether this device was not originally created on this stone.
    pub roaming: bool,
    /// Stone that created this seed bank.
    pub origin_stone: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Visibility setting.
    pub visibility: garden_common::storage::SeedBankVisibility,

    // --- Domain state (mutable) ---
    /// Runtime role (Primary / Dormant). Set by orchestration.
    pub role: SeedBankRole,
    /// Pin state — `Some` means Primary role is locked.
    pub pin: Option<PinState>,

    // --- Infrastructure (composed) ---
    /// Physical device lifecycle (mount, health, capacity).
    pub storage: StorageDevice,
    /// I/O chokepoint (filesystem reads/writes, encryption).
    pub store: SeedBankStore,
}

impl SeedBank {
    /// Construct from a detected + mounted storage device and its manifest.
    ///
    /// The `StorageDevice` must already be in `Healthy` state (post-detection).
    /// Pin state is loaded from disk during construction.
    pub async fn from_storage(
        storage: StorageDevice,
        manifest: &SeedBankManifest,
        dek: Option<[u8; 32]>,
    ) -> Self {
        let stone_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let roaming = manifest.origin_stone != stone_name;

        let store = SeedBankStore::new(storage.mount_path.clone(), dek);

        // Load persisted pin from disk
        let pin = match store.read_pin().await {
            Some(pin_id) => {
                info!(
                    name = %manifest.name,
                    pin_id = %pin_id,
                    "Loaded persisted pin from seed bank"
                );
                Some(PinState { pin_id })
            }
            None => None,
        };

        let id = manifest.id.clone();
        let short_id = SeedBankInfo::short_id(&id);

        Self {
            id,
            short_id,
            name: manifest.name.clone(),
            encrypted: manifest.encrypted,
            roaming,
            origin_stone: manifest.origin_stone.clone(),
            created_at: manifest.created_at,
            visibility: manifest.visibility,
            role: SeedBankRole::default(), // orchestration assigns later
            pin,
            storage,
            store,
        }
    }

    // ========================================================================
    // Pin operations
    // ========================================================================

    /// Pin this bank as Primary. Verifies mount, writes pin.json, updates role.
    ///
    /// Returns the generated GUIDv7 pin_id.
    pub async fn pin(&mut self) -> anyhow::Result<String> {
        // Gate: verify mount before writing
        self.storage.ensure_mounted().await?;

        let pin_id = uuid::Uuid::now_v7().to_string();
        self.store.write_pin(&pin_id).await?;
        self.pin = Some(PinState {
            pin_id: pin_id.clone(),
        });
        self.role = SeedBankRole::Primary;

        info!(name = %self.name, pin_id = %pin_id, "Seed bank pinned — claiming Primary");
        Ok(pin_id)
    }

    /// Remove the pin, returning to normal orchestration.
    ///
    /// Returns the old pin_id if one was set.
    pub async fn unpin(&mut self) -> anyhow::Result<Option<String>> {
        let old = self.pin.take();
        if let Some(ref ps) = old {
            // Best-effort delete from disk (may fail if mount is lost)
            if let Err(e) = self.store.delete_pin().await {
                warn!(name = %self.name, error = %e, "Failed to delete pin file from disk");
            }
            info!(name = %self.name, pin_id = %ps.pin_id, "Seed bank unpinned");
        }
        Ok(old.map(|ps| ps.pin_id))
    }

    /// Whether the Primary role is pinned.
    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    /// The pin_id if pinned, or `None`.
    pub fn pin_id(&self) -> Option<&str> {
        self.pin.as_ref().map(|ps| ps.pin_id.as_str())
    }

    // ========================================================================
    // Health
    // ========================================================================

    /// Run periodic health check — call from coordinator tick (~10s).
    ///
    /// Delegates to `StorageDevice::health_tick()` for mount/capacity,
    /// then reconciles domain state (e.g. detects external pin changes).
    pub async fn health_tick(&mut self) {
        self.storage.health_tick().await;

        // If healthy, reconcile pin state from disk (detect external changes)
        if self.storage.health.is_usable() {
            self.reconcile_pin().await;
        }
    }

    /// Re-read pin.json from disk to detect external changes.
    async fn reconcile_pin(&mut self) {
        let disk_pin = self.store.read_pin().await;
        match (&self.pin, &disk_pin) {
            (Some(current), Some(on_disk)) if current.pin_id != *on_disk => {
                debug!(
                    name = %self.name,
                    in_memory = %current.pin_id,
                    on_disk = %on_disk,
                    "Pin reconciliation: disk differs — adopting disk version"
                );
                self.pin = Some(PinState {
                    pin_id: on_disk.clone(),
                });
            }
            (None, Some(on_disk)) => {
                debug!(
                    name = %self.name,
                    pin_id = %on_disk,
                    "Pin reconciliation: disk has pin not in memory — adopting"
                );
                self.pin = Some(PinState {
                    pin_id: on_disk.clone(),
                });
            }
            (Some(current), None) => {
                debug!(
                    name = %self.name,
                    pin_id = %current.pin_id,
                    "Pin reconciliation: memory has pin not on disk — clearing"
                );
                self.pin = None;
            }
            _ => {} // in sync
        }
    }

    // ========================================================================
    // Projection to SeedBankInfo (for API / beacon compat)
    // ========================================================================

    /// Project this lifecycle object into a `SeedBankInfo` for API responses
    /// and beacon construction.
    pub fn to_info(&self) -> SeedBankInfo {
        SeedBankInfo::new(
            self.id.clone(),
            self.name.clone(),
            self.storage.device.clone(),
            self.storage.mount_path.to_string_lossy().to_string(),
            self.storage.capacity_bytes,
            self.storage.used_bytes,
            self.visibility,
            self.storage.filesystem == "btrfs",
            self.origin_stone.clone(),
            self.created_at,
            self.roaming,
            self.storage.health.is_usable(),
            self.encrypted,
        )
    }

    /// Build a `SeedBankSummary` for CLI display and portrait enrichment.
    pub fn to_summary(&self, stone_name: Option<&str>) -> garden_common::storage::SeedBankSummary {
        garden_common::storage::SeedBankSummary::from_info(
            &self.to_info(),
            self.role,
            self.is_pinned(),
            stone_name,
        )
    }
}

// ============================================================================
// SeedBanks collection type alias
// ============================================================================

/// The unified seed bank collection — keyed by seed bank ID (GUIDv7).
///
/// Single source of truth for all local seed bank state. Replaces the
/// scattered `seed_bank_cache`, `seed_bank_roles`, `seed_bank_pins`,
/// and `mount_tracker` collections.
pub type SeedBanks = std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, SeedBank>>>;

/// Create an empty `SeedBanks` map.
pub fn new_seed_banks() -> SeedBanks {
    std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()))
}
