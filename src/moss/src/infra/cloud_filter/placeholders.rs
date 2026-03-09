//! Placeholder creation helpers (STORAGE-0012)
//!
//! Shared by both the CfApi callback path (`provider.rs`) and the proactive
//! storage watcher (`mod.rs`).  Uses `nt_time::FileTime::now()` with the
//! native `Metadata::created()/written()` methods — matching the exact pattern
//! used in the `cloud-filter` crate's own integration tests.

use std::path::Path;

use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder_file::PlaceholderFile;
use nt_time::FileTime;
use tracing::{info, warn};

// ============================================================================
// Placeholder CRUD
// ============================================================================

/// Create a placeholder directory for a storage under the sync root.
///
/// Uses `CfCreatePlaceholders` so Explorer picks it up immediately without
/// requiring the user to close and reopen the Zen Garden folder.
pub fn create_storage_placeholder(sync_root_path: &Path, name: &str) {
    let placeholder = build_placeholder(name, true, 0);

    match placeholder.create::<&Path>(sync_root_path) {
        Ok(usn) => {
            info!(storage = %name, usn, "placeholder directory created");
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

/// Build a `PlaceholderFile` for a directory entry.
///
/// Mirrors the exact pattern from the `cloud-filter` crate's working async
/// test: `FileTime::now()` + native `Metadata::created().written()` +
/// `.blob()` + `.mark_in_sync()`.
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
