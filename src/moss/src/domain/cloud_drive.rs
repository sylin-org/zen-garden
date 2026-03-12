//! Cloud Drive domain policy (STORAGE-0015)
//!
//! Pure decision functions for the Cloud Filter rename/move tree.
//! No I/O, no CfApi types, no filesystem access — fully unit testable.

use std::path::{Path, PathBuf};

/// The action the Cloud Filter adapter should take after a rename callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveAction {
    /// File dragged from outside sync root into a storage folder.
    IngestFromOutside {
        source: PathBuf,
        storage: String,
        path: String,
        is_dir: bool,
    },
    /// File moved out of sync root (e.g. to Recycle Bin) — delete from mount.
    DeleteFromStorage {
        storage: String,
        path: String,
        is_dir: bool,
    },
    /// Rename/move within the same storage.
    RenameInStorage {
        storage: String,
        old: String,
        new: String,
    },
    /// Move between two known storages (copy + delete).
    CrossStorageMove {
        src_storage: String,
        src: String,
        dst_storage: String,
        dst: String,
        is_dir: bool,
    },
    /// Top-level storage folder renamed (replica set name change).
    RenameStorage {
        old_name: String,
        new_name: String,
    },
    /// Stray root item moved into a storage (not a known storage name).
    IngestStray {
        stray_path: PathBuf,
        storage: String,
        path: String,
        is_dir: bool,
    },
    /// Operation is not supported — reject the rename.
    Reject {
        reason: &'static str,
    },
}

/// Classify a CfApi rename callback into a domain action.
///
/// All seven branches of the rename decision tree, as a pure function.
/// The caller resolves CfApi paths to `(storage_name, rel_path)` tuples
/// before calling, and checks `is_known_storage` for the source.
///
/// # Arguments
///
/// * `source_in_scope`  — CfApi: is the source inside the sync root?
/// * `target_in_scope`  — CfApi: is the target inside the sync root?
/// * `old_storage`      — resolved source storage name (empty if at root)
/// * `old_rel`          — resolved source relative path within storage
/// * `new_storage`      — resolved target storage name (empty if at root)
/// * `new_rel`          — resolved target relative path within storage
/// * `is_dir`           — whether the item being moved is a directory
/// * `is_known_storage` — whether `old_storage` is a known (local or remote) storage
/// * `source_path`      — original filesystem path of the source
/// * `sync_root_path`   — the sync root base path
#[allow(clippy::too_many_arguments)]
pub fn classify_rename(
    source_in_scope: bool,
    target_in_scope: bool,
    old_storage: &str,
    old_rel: &str,
    new_storage: &str,
    new_rel: &str,
    is_dir: bool,
    is_known_storage: bool,
    source_path: &Path,
    sync_root_path: &Path,
) -> DriveAction {
    // 1. Drag from outside sync root into a storage
    if !source_in_scope && target_in_scope {
        if new_storage.is_empty() || new_rel.is_empty() {
            return DriveAction::Reject {
                reason: "Cannot drop items at sync root level",
            };
        }
        return DriveAction::IngestFromOutside {
            source: source_path.to_path_buf(),
            storage: new_storage.to_string(),
            path: new_rel.to_string(),
            is_dir,
        };
    }

    // 2. Move out of sync root (e.g. Recycle Bin)
    if source_in_scope && !target_in_scope {
        if old_storage.is_empty() || old_rel.is_empty() {
            return DriveAction::Reject {
                reason: "Cannot move storage root out of sync root",
            };
        }
        return DriveAction::DeleteFromStorage {
            storage: old_storage.to_string(),
            path: old_rel.to_string(),
            is_dir,
        };
    }

    // 3. Source at sync root level (no storage context)
    if old_storage.is_empty() {
        return DriveAction::Reject {
            reason: "Source has no storage context",
        };
    }

    // 4. Top-level storage rename (replica set name change)
    if old_rel.is_empty() && new_rel.is_empty() && old_storage != new_storage {
        return DriveAction::RenameStorage {
            old_name: old_storage.to_string(),
            new_name: new_storage.to_string(),
        };
    }

    // 5–6. Cross-storage operations
    if old_storage != new_storage {
        if !is_known_storage {
            // 5. Stray root item moved into a storage
            let stray_path = sync_root_path.join(old_storage).join(old_rel);
            let dst_path = if new_rel.is_empty() {
                old_storage.to_string()
            } else {
                new_rel.to_string()
            };
            return DriveAction::IngestStray {
                stray_path,
                storage: new_storage.to_string(),
                path: dst_path,
                is_dir,
            };
        }

        // 6. Cross-storage move between two known storages
        return DriveAction::CrossStorageMove {
            src_storage: old_storage.to_string(),
            src: old_rel.to_string(),
            dst_storage: new_storage.to_string(),
            dst: new_rel.to_string(),
            is_dir,
        };
    }

    // 7. Within-storage rename
    if !old_rel.is_empty() && !new_rel.is_empty() {
        return DriveAction::RenameInStorage {
            storage: old_storage.to_string(),
            old: old_rel.to_string(),
            new: new_rel.to_string(),
        };
    }

    DriveAction::Reject {
        reason: "Unclassified rename operation",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sr() -> PathBuf {
        PathBuf::from("C:\\Users\\test\\Zen Garden")
    }

    #[test]
    fn test_ingest_from_outside() {
        let src = PathBuf::from("C:\\Users\\test\\Desktop\\photo.jpg");
        let action = classify_rename(
            false, true,
            "", "",
            "photos", "photo.jpg",
            false, false,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::IngestFromOutside { ref storage, ref path, .. }
            if storage == "photos" && path == "photo.jpg"));
    }

    #[test]
    fn test_ingest_from_outside_to_root_rejected() {
        let src = PathBuf::from("C:\\Users\\test\\Desktop\\photo.jpg");
        let action = classify_rename(
            false, true,
            "", "",
            "", "",
            false, false,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::Reject { .. }));
    }

    #[test]
    fn test_delete_from_storage() {
        let src = sr().join("photos").join("cats.jpg");
        let action = classify_rename(
            true, false,
            "photos", "cats.jpg",
            "", "",
            false, true,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::DeleteFromStorage { ref storage, ref path, .. }
            if storage == "photos" && path == "cats.jpg"));
    }

    #[test]
    fn test_rename_storage() {
        let src = sr().join("photos");
        let action = classify_rename(
            true, true,
            "photos", "",
            "pictures", "",
            true, true,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::RenameStorage { ref old_name, ref new_name }
            if old_name == "photos" && new_name == "pictures"));
    }

    #[test]
    fn test_cross_storage_move() {
        let src = sr().join("photos").join("cats.jpg");
        let action = classify_rename(
            true, true,
            "photos", "cats.jpg",
            "backups", "cats.jpg",
            false, true,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::CrossStorageMove {
            ref src_storage, ref src, ref dst_storage, ref dst, ..
        } if src_storage == "photos" && src == "cats.jpg"
           && dst_storage == "backups" && dst == "cats.jpg"));
    }

    #[test]
    fn test_ingest_stray() {
        let src = sr().join("stray-folder").join("file.txt");
        let action = classify_rename(
            true, true,
            "stray-folder", "file.txt",
            "photos", "file.txt",
            false, false,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::IngestStray { ref storage, ref path, .. }
            if storage == "photos" && path == "file.txt"));
    }

    #[test]
    fn test_rename_in_storage() {
        let src = sr().join("photos").join("old.jpg");
        let action = classify_rename(
            true, true,
            "photos", "old.jpg",
            "photos", "new.jpg",
            false, true,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::RenameInStorage {
            ref storage, ref old, ref new
        } if storage == "photos" && old == "old.jpg" && new == "new.jpg"));
    }

    #[test]
    fn test_source_at_root_rejected() {
        let src = sr().join("something");
        let action = classify_rename(
            true, true,
            "", "something",
            "photos", "something",
            false, false,
            &src, &sr(),
        );
        assert!(matches!(action, DriveAction::Reject { .. }));
    }
}
