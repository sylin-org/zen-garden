//! Placeholder creation and state-management helpers (STORAGE-0012, STORAGE-0016)
//!
//! Shared by both the CfApi callback path (`provider.rs`) and the proactive
//! storage watcher (`mod.rs`).  Uses `nt_time::FileTime::now()` with the
//! native `Metadata::created()/written()` methods — matching the exact pattern
//! used in the `cloud-filter` crate's own integration tests.
//!
//! ## Availability signalling (STORAGE-0016)
//!
//! Storage root directories (one per replica set) carry an IN_SYNC flag and a
//! structured blob so Explorer can show the right overlay:
//!
//! | State | Flag | Explorer icon |
//! |-------|------|---------------|
//! | Online (local or remote) | IN_SYNC | ✓ green checkmark |
//! | Offline (no stone reachable) | not IN_SYNC | ↑ cloud pending |
//!
//! File and sub-directory placeholders inside a storage are always created
//! IN_SYNC — they are only created when the storage is accessible.

use std::path::Path;

use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder_file::PlaceholderFile;
use nt_time::FileTime;
use tracing::{debug, info, warn};

// ============================================================================
// Availability metadata
// ============================================================================

/// Availability state for a storage replica set.
///
/// Carried as a structured blob inside the root placeholder so `mod.rs` and
/// `signaling.rs` can retrieve stone context for notifications and info-bar
/// messages without re-querying the registry.
#[derive(Debug, Clone)]
pub(crate) struct StorageAvailability {
    /// Whether any stone hosting this replica set is currently reachable.
    pub online: bool,
    /// Display name of the stone currently hosting the storage.
    /// `"this device"` when the storage is physically on this stone.
    pub stone_name: String,
    /// `true` when the storage bank is mounted on the local stone's filesystem.
    pub local: bool,
}

impl StorageAvailability {
    pub(crate) fn online(stone_name: impl Into<String>, local: bool) -> Self {
        Self { online: true, stone_name: stone_name.into(), local }
    }

}

// ============================================================================
// Placeholder CRUD
// ============================================================================

/// Create a placeholder directory for a storage under the sync root.
///
/// Uses `CfCreatePlaceholders` so Explorer picks it up immediately without
/// requiring the user to close and reopen the Zen Garden folder.
pub fn create_storage_placeholder(
    sync_root_path: &Path,
    name: &str,
    avail: &StorageAvailability,
) {
    let placeholder = build_storage_dir_placeholder(name, avail);

    match placeholder.create::<&Path>(sync_root_path) {
        Ok(usn) => {
            info!(storage = %name, online = avail.online, usn, "placeholder directory created");
        }
        Err(e) => {
            // Not fatal — the placeholder may already exist from a prior
            // fetch_placeholders callback, or the sync root may not be ready.
            warn!(storage = %name, error = %e, "failed to create placeholder directory");
        }
    }
}

/// Remove a placeholder directory for a departed storage.
pub async fn remove_storage_placeholder(sync_root_path: &Path, name: &str) {
    let dir = sync_root_path.join(name);
    if dir.exists() {
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => info!(storage = %name, "placeholder directory removed"),
            Err(e) => warn!(storage = %name, error = %e, "failed to remove placeholder directory"),
        }
    }
}

/// Update the IN_SYNC flag on an existing storage root placeholder.
///
/// Called by the reconciler whenever a storage transitions between online and
/// offline states. No-op if the placeholder directory does not exist.
pub fn update_storage_placeholder_state(sync_root_path: &Path, name: &str, online: bool) {
    use cloud_filter::placeholder::Placeholder;

    let path = sync_root_path.join(name);
    if !path.exists() {
        return;
    }

    match Placeholder::open(&path) {
        Ok(mut ph) => {
            if let Err(e) = ph.mark_in_sync(online, None) {
                warn!(
                    storage = %name,
                    online,
                    error = %e,
                    "failed to update placeholder sync state"
                );
            } else {
                debug!(storage = %name, online, "placeholder sync state updated");
            }
        }
        Err(e) => {
            debug!(storage = %name, error = %e, "could not open placeholder for state update");
        }
    }
}

// ============================================================================
// Placeholder builders
// ============================================================================

/// Build a `PlaceholderFile` for a **storage root directory**.
///
/// Sets IN_SYNC based on availability and encodes the stone context in the
/// blob for use by `signaling.rs` at notification time.
pub fn build_storage_dir_placeholder(name: &str, avail: &StorageAvailability) -> PlaceholderFile {
    let now = FileTime::now();
    let blob = encode_storage_blob(name, avail);

    let ph = PlaceholderFile::new(name)
        .metadata(Metadata::directory().created(now).written(now).size(0))
        .blob(blob);

    if avail.online {
        ph.mark_in_sync()
    } else {
        ph
    }
}

/// Build a `PlaceholderFile` for a **file or sub-directory inside a storage**.
///
/// Always marked IN_SYNC — these are only created when the storage is
/// accessible, so the data is valid at creation time.
pub fn build_placeholder(name: &str, is_dir: bool, size: u64) -> PlaceholderFile {
    let now = FileTime::now();

    if is_dir {
        PlaceholderFile::new(name)
            .mark_in_sync()
            .metadata(
                Metadata::directory()
                    .created(now)
                    .written(now)
                    .size(0),
            )
            .blob(name.into())
    } else {
        PlaceholderFile::new(name)
            .has_no_children()
            .mark_in_sync()
            .metadata(
                Metadata::file()
                    .created(now)
                    .written(now)
                    .size(size),
            )
            .blob(name.into())
    }
}

// ============================================================================
// Blob encoding
// ============================================================================

/// Encode storage availability metadata as compact JSON bytes for the blob.
///
/// Schema: `{"n":"storage","s":"stone-golden-summit","l":false}`
///
/// - `n` = replica set display name
/// - `s` = stone name (empty when offline)
/// - `l` = local (true = bank is mounted on this stone)
fn encode_storage_blob(name: &str, avail: &StorageAvailability) -> Vec<u8> {
    let json = serde_json::json!({
        "n": name,
        "s": avail.stone_name,
        "l": avail.local,
    });
    json.to_string().into_bytes()
}
