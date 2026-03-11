//! Storage on-disk layout management (STORAGE-0009)
//!
//! Owns the `.zen-garden/` dotfolder structure on managed storage devices:
//! - Directory initialization (prepare, adopt)
//! - `Zen Garden` symlink at mount root
//! - Migration from legacy `garden/` to `.zen-garden/`
//! - Layout validation

use std::path::Path;

use anyhow::{Context, Result};
use garden_common::constants::paths;
use tracing::{debug, info, warn};

// ============================================================================
// Layout initialization
// ============================================================================

/// Initialize the `.zen-garden/` dotfolder structure on a managed storage.
///
/// Creates all required subdirectories and the `Zen Garden` symlink.
/// Called during both `prepare` (blank device) and `adopt` (populated device).
pub async fn initialize_layout(mount_path: &Path) -> Result<()> {
    let dotfolder = mount_path.join(paths::STORAGE_DOTFOLDER);
    tokio::fs::create_dir_all(&dotfolder)
        .await
        .context("Failed to create .zen-garden directory")?;

    // Memories directory (for seed-bank role)
    let memories = mount_path.join(paths::STORAGE_MEMORIES_DIR);
    tokio::fs::create_dir_all(&memories)
        .await
        .context("Failed to create memories directory")?;

    // Object storage directory
    let objects = mount_path.join(paths::STORAGE_OBJECTS_DIR);
    tokio::fs::create_dir_all(&objects)
        .await
        .context("Failed to create storage directory")?;

    // Last-known-good directory
    let lkg = mount_path.join(paths::STORAGE_LAST_KNOWN_GOOD_DIR);
    tokio::fs::create_dir_all(&lkg)
        .await
        .context("Failed to create last-known-good directory")?;

    // Zen Garden symlink
    create_symlink(mount_path).await;

    debug!(path = %mount_path.display(), "Storage layout initialized");
    Ok(())
}

// ============================================================================
// Zen Garden symlink
// ============================================================================

/// Create the `Zen Garden` symlink at mount root pointing to `.zen-garden/`.
///
/// The symlink gives users visibility into managed data from their file
/// explorer. Present by default; users can delete it — Moss does not
/// recreate it unless asked.
///
/// On Windows, uses a junction point (no elevated privileges required)
/// instead of a symlink.
pub async fn create_symlink(mount_path: &Path) {
    let link = mount_path.join("Zen Garden");
    let target = mount_path.join(paths::STORAGE_DOTFOLDER);

    // Don't recreate if it already exists (user may have deleted it intentionally)
    if link.exists() {
        return;
    }

    // Ensure target exists
    if !target.is_dir() {
        return;
    }

    #[cfg(unix)]
    {
        if let Err(e) = tokio::fs::symlink(&target, &link).await {
            warn!(
                link = %link.display(),
                error = %e,
                "Failed to create Zen Garden symlink"
            );
        } else {
            info!(link = %link.display(), "Created Zen Garden symlink");
        }
    }

    #[cfg(windows)]
    {
        // Use junction point — no elevated privileges required on Windows
        if let Err(e) = std::os::windows::fs::symlink_dir(&target, &link) {
            // Fall back to junction via cmd /c mklink /J
            let link_str = link.to_string_lossy();
            let target_str = target.to_string_lossy();
            match tokio::process::Command::new("cmd")
                .args(["/c", "mklink", "/J", &link_str, &target_str])
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    info!(link = %link.display(), "Created Zen Garden junction point");
                }
                _ => {
                    warn!(
                        link = %link.display(),
                        error = %e,
                        "Failed to create Zen Garden symlink or junction"
                    );
                }
            }
        } else {
            info!(link = %link.display(), "Created Zen Garden symlink");
        }
    }
}

// ============================================================================
// Layout validation
// ============================================================================

/// Validate that a mount point has the expected `.zen-garden/` structure.
///
/// Returns `Ok(())` if the layout is valid, or an error message describing
/// what's missing.
pub fn validate_layout(mount_path: &Path) -> Result<(), String> {
    let dotfolder = mount_path.join(paths::STORAGE_DOTFOLDER);
    if !dotfolder.is_dir() {
        return Err(format!(
            "Missing {} directory at {}",
            paths::STORAGE_DOTFOLDER,
            mount_path.display()
        ));
    }

    let manifest = dotfolder.join("manifest.json");
    if !manifest.is_file() {
        return Err(format!(
            "Missing manifest.json in {}",
            dotfolder.display()
        ));
    }

    Ok(())
}

// ============================================================================
// Legacy migration: garden/ -> .zen-garden/
// ============================================================================

/// Migrate a storage from the legacy `garden/` layout to `.zen-garden/`.
///
/// Checks if the mount has a `garden/` directory with the old layout
/// but no `.zen-garden/` directory, and performs an in-place migration:
///
/// 1. Rename `garden/` to `.zen-garden/`
/// 2. Create the `Zen Garden` symlink
/// 3. Create any new directories (`last-known-good/`)
///
/// Returns `true` if migration was performed, `false` if no migration needed.
pub async fn migrate_legacy_layout(mount_path: &Path) -> Result<bool> {
    let legacy = mount_path.join("garden");
    let dotfolder = mount_path.join(paths::STORAGE_DOTFOLDER);

    // No legacy layout, or already migrated
    if !legacy.is_dir() || dotfolder.is_dir() {
        return Ok(false);
    }

    // Verify legacy layout has a manifest
    let legacy_manifest = legacy.join("manifest.json");
    if !legacy_manifest.is_file() {
        debug!(
            path = %mount_path.display(),
            "Legacy garden/ directory exists but has no manifest — skipping migration"
        );
        return Ok(false);
    }

    info!(
        path = %mount_path.display(),
        "Migrating legacy garden/ layout to .zen-garden/"
    );

    // Rename garden/ -> .zen-garden/
    tokio::fs::rename(&legacy, &dotfolder)
        .await
        .context("Failed to rename garden/ to .zen-garden/")?;

    // Create new directories that didn't exist in the legacy layout
    let lkg = mount_path.join(paths::STORAGE_LAST_KNOWN_GOOD_DIR);
    let _ = tokio::fs::create_dir_all(&lkg).await;

    // Ensure memories/ and storage/ exist (they should from the rename)
    let memories = mount_path.join(paths::STORAGE_MEMORIES_DIR);
    let _ = tokio::fs::create_dir_all(&memories).await;

    let objects = mount_path.join(paths::STORAGE_OBJECTS_DIR);
    let _ = tokio::fs::create_dir_all(&objects).await;

    // Create Zen Garden symlink
    create_symlink(mount_path).await;

    info!(
        path = %mount_path.display(),
        "Legacy migration complete: garden/ -> .zen-garden/"
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_initialize_layout() {
        let tmp = TempDir::new().unwrap();
        let mount = tmp.path();

        initialize_layout(mount).await.unwrap();

        assert!(mount.join(paths::STORAGE_DOTFOLDER).is_dir());
        assert!(mount.join(paths::STORAGE_MEMORIES_DIR).is_dir());
        assert!(mount.join(paths::STORAGE_OBJECTS_DIR).is_dir());
        assert!(mount.join(paths::STORAGE_LAST_KNOWN_GOOD_DIR).is_dir());
    }

    #[tokio::test]
    async fn test_validate_layout_missing_dotfolder() {
        let tmp = TempDir::new().unwrap();
        assert!(validate_layout(tmp.path()).is_err());
    }

    #[tokio::test]
    async fn test_validate_layout_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(paths::STORAGE_DOTFOLDER)).unwrap();
        assert!(validate_layout(tmp.path()).is_err());
    }

    #[tokio::test]
    async fn test_validate_layout_ok() {
        let tmp = TempDir::new().unwrap();
        let dotfolder = tmp.path().join(paths::STORAGE_DOTFOLDER);
        std::fs::create_dir_all(&dotfolder).unwrap();
        std::fs::write(dotfolder.join("manifest.json"), "{}").unwrap();
        assert!(validate_layout(tmp.path()).is_ok());
    }

    #[tokio::test]
    async fn test_migrate_legacy_layout() {
        let tmp = TempDir::new().unwrap();
        let mount = tmp.path();

        // Create legacy layout
        let legacy = mount.join("garden");
        std::fs::create_dir_all(legacy.join("memories")).unwrap();
        std::fs::create_dir_all(legacy.join("storage")).unwrap();
        std::fs::write(legacy.join("manifest.json"), r#"{"version":3}"#).unwrap();

        // Run migration
        let migrated = migrate_legacy_layout(mount).await.unwrap();
        assert!(migrated);

        // Verify new layout
        assert!(!mount.join("garden").is_dir());
        assert!(mount.join(paths::STORAGE_DOTFOLDER).is_dir());
        assert!(mount.join(paths::STORAGE_MEMORIES_DIR).is_dir());
        assert!(mount.join(paths::STORAGE_OBJECTS_DIR).is_dir());
        assert!(mount.join(paths::STORAGE_LAST_KNOWN_GOOD_DIR).is_dir());
    }

    #[tokio::test]
    async fn test_migrate_no_legacy() {
        let tmp = TempDir::new().unwrap();
        let migrated = migrate_legacy_layout(tmp.path()).await.unwrap();
        assert!(!migrated);
    }

    #[tokio::test]
    async fn test_migrate_already_migrated() {
        let tmp = TempDir::new().unwrap();
        let mount = tmp.path();

        // Create both old and new
        std::fs::create_dir_all(mount.join("garden")).unwrap();
        std::fs::create_dir_all(mount.join(paths::STORAGE_DOTFOLDER)).unwrap();

        let migrated = migrate_legacy_layout(mount).await.unwrap();
        assert!(!migrated);
    }
}
