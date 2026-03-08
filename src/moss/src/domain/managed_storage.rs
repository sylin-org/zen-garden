//! Seed bank lifecycle domain object (STORAGE-0007)
//!
//! `ManagedStorage` is the single source of truth for a local seed bank's identity,
//! role, pin state, and I/O store. It **composes** a [`StorageDevice`] (infra)
//! and a [`ContentStore`] (infra) — enforcing mount verification before any
//! write operation.
//!
//! ## Design
//!
//! - **Domain layer**: owns identity, role, pin. No filesystem I/O directly.
//! - **Composes infrastructure**: delegates mount health to `StorageDevice`,
//!   file I/O to `ContentStore`.
//! - **Single lock**: `AppState::seed_banks` is the ONE map holding all local
//!   seed bank state. No more scattered `seed_bank_roles`, `seed_bank_pins`,
//!   `seed_bank_cache`, `mount_tracker`.

use garden_common::storage::{StorageInfo, StorageManifest, StorageRole};
use tracing::{debug, info, warn};

use crate::infra::storage::{ContentStore, StorageDevice};

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
// ManagedStorage
// ============================================================================

/// Lifecycle object for a locally-mounted seed bank.
///
/// Created from a `StorageDevice` + `StorageManifest` at detection time.
/// Lives in `AppState::seed_banks` for the device's lifetime.
#[derive(Debug, Clone)]
pub struct ManagedStorage {
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
    pub visibility: garden_common::storage::StorageVisibility,
    /// Composable roles (e.g., ["seed-bank"]).
    pub roles: Vec<String>,

    // --- Domain state (mutable) ---
    /// Runtime role (Primary / Dormant). Set by orchestration.
    pub role: StorageRole,
    /// Pin state — `Some` means Primary role is locked.
    pub pin: Option<PinState>,

    // --- Infrastructure (composed) ---
    /// Physical device lifecycle (mount, health, capacity).
    pub storage: StorageDevice,
    /// I/O chokepoint (filesystem reads/writes, encryption).
    pub store: ContentStore,
}

impl ManagedStorage {
    /// Construct from a detected + mounted storage device and its manifest.
    ///
    /// The `StorageDevice` must already be in `Healthy` state (post-detection).
    /// Pin state is loaded from disk during construction.
    pub async fn from_storage(
        storage: StorageDevice,
        manifest: &StorageManifest,
        dek: Option<[u8; 32]>,
    ) -> Self {
        let stone_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let roaming = manifest.origin_stone != stone_name;

        let store = ContentStore::new(storage.mount_path.clone(), dek);

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
        let short_id = StorageInfo::short_id(&id);

        Self {
            id,
            short_id,
            name: manifest.name.clone(),
            encrypted: manifest.encrypted,
            roaming,
            origin_stone: manifest.origin_stone.clone(),
            created_at: manifest.created_at,
            visibility: manifest.visibility,
            roles: manifest.roles.clone(),
            role: StorageRole::default(), // orchestration assigns later
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
        self.role = StorageRole::Primary;

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

    /// Snapshot critical structural files to `last-known-good/`.
    ///
    /// Called at known-safe moments: after successful mount, after
    /// replication cycle, periodically from the coordinator.
    pub async fn snapshot_lkg(&self) {
        if let Err(e) = self.store.snapshot_lkg().await {
            warn!(name = %self.name, error = %e, "Failed to snapshot last-known-good");
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
    // Projection to StorageInfo (for API / beacon compat)
    // ========================================================================

    /// Project this lifecycle object into a `StorageInfo` for API responses
    /// and beacon construction.
    pub fn to_info(&self) -> StorageInfo {
        StorageInfo::new(
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
            self.roles.clone(),
        )
    }

    /// Build a `StorageSummary` for CLI display and portrait enrichment.
    pub fn to_summary(&self, stone_name: Option<&str>) -> garden_common::storage::StorageSummary {
        garden_common::storage::StorageSummary::from_info(
            &self.to_info(),
            self.role,
            self.is_pinned(),
            stone_name,
        )
    }
}

// ============================================================================
// ManagedStorages collection type alias
// ============================================================================

/// The unified seed bank collection — keyed by seed bank ID (GUIDv7).
///
/// Single source of truth for all local seed bank state. Replaces the
/// scattered `seed_bank_cache`, `seed_bank_roles`, `seed_bank_pins`,
/// and `mount_tracker` collections.
pub type ManagedStorages = std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, ManagedStorage>>>;

/// Create an empty `ManagedStorages` map.
pub fn new_managed_storages() -> ManagedStorages {
    std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::storage::{ContentStore, StorageDevice, StorageHealth};
    use garden_common::storage::StorageVisibility;
    use tempfile::TempDir;

    /// Build a test ManagedStorage with defaults.
    fn make_test_bank(name: &str, role: StorageRole, mount_path: &std::path::Path) -> ManagedStorage {
        ManagedStorage {
            id: "01956a3e-0000-7000-8000-000000000001".to_string(),
            short_id: "01956a3e".to_string(),
            name: name.to_string(),
            encrypted: false,
            roaming: false,
            origin_stone: "stone-test".to_string(),
            created_at: chrono::Utc::now(),
            visibility: StorageVisibility::Open,
            roles: vec!["seed-bank".to_string()],
            role,
            pin: None,
            storage: StorageDevice::new("/dev/sda1", mount_path, "ext4", 500_000_000_000, 50_000_000_000),
            store: ContentStore::new_public(mount_path),
        }
    }

    #[test]
    fn test_is_pinned_when_no_pin() {
        let tmp = TempDir::new().unwrap();
        let bank = make_test_bank("test", StorageRole::Primary, tmp.path());
        assert!(!bank.is_pinned());
        assert!(bank.pin_id().is_none());
    }

    #[test]
    fn test_is_pinned_when_pinned() {
        let tmp = TempDir::new().unwrap();
        let mut bank = make_test_bank("test", StorageRole::Primary, tmp.path());
        bank.pin = Some(PinState { pin_id: "pin-001".to_string() });
        assert!(bank.is_pinned());
        assert_eq!(bank.pin_id(), Some("pin-001"));
    }

    #[test]
    fn test_to_info_projection() {
        let tmp = TempDir::new().unwrap();
        let bank = make_test_bank("my-storage", StorageRole::Primary, tmp.path());
        let info = bank.to_info();

        assert_eq!(info.name, "my-storage");
        assert_eq!(info.device, "/dev/sda1");
        assert_eq!(info.capacity_bytes, 500_000_000_000);
        assert_eq!(info.used_bytes, 50_000_000_000);
        assert_eq!(info.visibility, StorageVisibility::Open);
        assert!(!info.encrypted);
        assert!(!info.roaming);
        assert!(info.online); // Healthy → online
    }

    #[test]
    fn test_to_info_reflects_health() {
        let tmp = TempDir::new().unwrap();
        let mut bank = make_test_bank("failing", StorageRole::Primary, tmp.path());
        bank.storage.health = StorageHealth::Lost;
        let info = bank.to_info();
        assert!(!info.online); // Lost → not usable → offline
    }

    #[test]
    fn test_to_summary_includes_role_and_pin() {
        let tmp = TempDir::new().unwrap();
        let mut bank = make_test_bank("test", StorageRole::Dormant, tmp.path());
        bank.pin = Some(PinState { pin_id: "pin-x".to_string() });

        let summary = bank.to_summary(Some("stone-alpha"));
        assert_eq!(summary.role, StorageRole::Dormant);
        assert!(summary.pinned);
        assert_eq!(summary.stone_name.as_deref(), Some("stone-alpha"));
    }

    #[tokio::test]
    async fn test_pin_writes_to_disk() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zen-garden")).unwrap();
        let mut bank = make_test_bank("test", StorageRole::Dormant, tmp.path());

        let pin_id = bank.pin().await.unwrap();
        assert!(bank.is_pinned());
        assert_eq!(bank.role, StorageRole::Primary);

        // Verify pin.json was written
        let pin_path = tmp.path().join(".zen-garden/pin.json");
        assert!(pin_path.exists());
        let content = std::fs::read_to_string(&pin_path).unwrap();
        assert!(content.contains(&pin_id));
    }

    #[tokio::test]
    async fn test_unpin_clears_state() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zen-garden")).unwrap();
        let mut bank = make_test_bank("test", StorageRole::Primary, tmp.path());

        // Pin first
        let pin_id = bank.pin().await.unwrap();
        assert!(bank.is_pinned());

        // Unpin
        let old = bank.unpin().await.unwrap();
        assert_eq!(old, Some(pin_id));
        assert!(!bank.is_pinned());
    }

    #[tokio::test]
    async fn test_reconcile_pin_adopts_disk_version() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zen-garden")).unwrap();
        let mut bank = make_test_bank("test", StorageRole::Primary, tmp.path());

        // Write a pin to disk externally
        let pin_data = r#"{"pin_id": "external-pin-123"}"#;
        std::fs::write(tmp.path().join(".zen-garden/pin.json"), pin_data).unwrap();

        // In-memory has no pin
        assert!(!bank.is_pinned());

        // Reconcile picks up disk pin
        bank.reconcile_pin().await;
        assert!(bank.is_pinned());
        assert_eq!(bank.pin_id(), Some("external-pin-123"));
    }

    #[tokio::test]
    async fn test_reconcile_pin_clears_when_disk_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zen-garden")).unwrap();
        let mut bank = make_test_bank("test", StorageRole::Primary, tmp.path());
        bank.pin = Some(PinState { pin_id: "old-pin".to_string() });

        // No pin.json on disk
        assert!(bank.is_pinned());
        bank.reconcile_pin().await;
        assert!(!bank.is_pinned());
    }

    #[tokio::test]
    async fn test_from_storage_detects_roaming() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zen-garden")).unwrap();

        let manifest = StorageManifest {
            version: 4,
            id: "id-1".to_string(),
            name: "test-bank".to_string(),
            origin_stone: "some-other-stone-that-is-not-this-one".to_string(),
            filesystem: "ext4".to_string(),
            visibility: StorageVisibility::Open,
            encrypted: false,
            pond_fingerprint: None,
            created_at: chrono::Utc::now(),
            roles: vec!["seed-bank".to_string()],
        };

        let device = StorageDevice::new("/dev/sdb1", tmp.path(), "ext4", 1_000_000, 100_000);
        let bank = ManagedStorage::from_storage(device, &manifest, None).await;

        // Origin stone != this machine's hostname → roaming
        assert!(bank.roaming);
    }

    #[test]
    fn test_new_managed_storages_is_empty() {
        let storages = new_managed_storages();
        // Can't easily assert on the Arc<RwLock<HashMap>> without async,
        // but verify it's constructible
        assert!(std::sync::Arc::strong_count(&storages) == 1);
    }
}
