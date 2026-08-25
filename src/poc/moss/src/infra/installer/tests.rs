//! Test harnesses for the installer module
//!
//! Tests exercise package resolution, version breadcrumbs, install mode
//! detection, and file deployment logic using temp directories.
//! System-level operations (systemctl, apt-get, chpasswd) are not tested
//! here — they require integration tests on actual Linux targets.

#[cfg(test)]
mod version_tests {
    use crate::infra::installer::version::*;
    #[test]
    fn installed_version_new_sets_fields() {
        let v = InstalledVersion::new("0.2.100", InstallMethod::Install);
        assert_eq!(v.version, "0.2.100");
        assert_eq!(v.method, InstallMethod::Install);
        assert!(!v.installed_at.is_empty());
        assert!(!v.platform.is_empty());
    }

    #[test]
    fn install_method_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&InstallMethod::Install).unwrap(),
            "\"install\""
        );
        assert_eq!(
            serde_json::to_string(&InstallMethod::DeployApi).unwrap(),
            "\"deploy-api\""
        );
        assert_eq!(
            serde_json::to_string(&InstallMethod::PreStart).unwrap(),
            "\"pre-start\""
        );
    }

    #[test]
    fn install_method_deserializes_kebab_case() {
        let m: InstallMethod = serde_json::from_str("\"deploy-api\"").unwrap();
        assert_eq!(m, InstallMethod::DeployApi);
    }

    #[test]
    fn installed_version_roundtrip_json() {
        let original = InstalledVersion {
            version: "0.2.202603161200".to_string(),
            installed_at: "2026-03-16T12:00:00Z".to_string(),
            platform: "linux-x64".to_string(),
            method: InstallMethod::PreStart,
        };

        let json = serde_json::to_string_pretty(&original).unwrap();
        let parsed: InstalledVersion = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.installed_at, original.installed_at);
        assert_eq!(parsed.platform, original.platform);
        assert_eq!(parsed.method, original.method);
    }

    #[test]
    fn install_mode_display_fresh() {
        assert_eq!(InstallMode::Fresh.to_string(), "Fresh install");
    }

    #[test]
    fn install_mode_display_update() {
        let mode = InstallMode::Update {
            from: "0.2.100".to_string(),
            to: "0.2.200".to_string(),
        };
        assert_eq!(mode.to_string(), "Update (0.2.100 -> 0.2.200)");
    }

    #[test]
    fn install_mode_display_repair() {
        let mode = InstallMode::Repair {
            version: "0.2.100".to_string(),
        };
        assert_eq!(mode.to_string(), "Repair (0.2.100)");
    }

    /// Test breadcrumb write and read with a temp directory
    #[test]
    fn breadcrumb_write_and_read_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let breadcrumb_file = temp.path().join("installed-version.json");

        let version = InstalledVersion::new("0.2.999", InstallMethod::Install);
        let json = serde_json::to_string_pretty(&version).unwrap();
        std::fs::write(&breadcrumb_file, &json).unwrap();

        let contents = std::fs::read_to_string(&breadcrumb_file).unwrap();
        let parsed: InstalledVersion = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.version, "0.2.999");
        assert_eq!(parsed.method, InstallMethod::Install);
    }
}

#[cfg(test)]
mod package_tests {
    use std::fs;
    use std::path::Path;

    /// Helper: create a fake package file in a temp directory
    fn create_fake_package(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"fake-package-data").unwrap();
    }

    #[test]
    fn find_local_package_selects_latest_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        // Create two packages — the later version should be picked
        create_fake_package(dir, "zen-garden-0.2.100-linux-x64.tar.gz");
        create_fake_package(dir, "zen-garden-0.2.200-linux-x64.tar.gz");

        // Find all matching files manually (mirrors package.rs logic)
        let suffix = "-linux-x64.tar.gz";
        let mut candidates: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("zen-garden-") && name.ends_with(suffix)
            })
            .map(|e| e.path())
            .collect();

        candidates.sort();
        let selected = candidates.pop().unwrap();
        assert!(
            selected
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("0.2.200"),
            "Should select the latest version"
        );
    }

    #[test]
    fn find_local_package_returns_none_for_wrong_platform() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        create_fake_package(dir, "zen-garden-0.2.100-windows-x64.zip");

        // Search for linux packages
        let suffix = "-linux-x64.tar.gz";
        let candidates: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("zen-garden-") && name.ends_with(suffix)
            })
            .collect();

        assert!(candidates.is_empty(), "Should not match wrong platform");
    }

    #[test]
    fn find_local_package_ignores_non_package_files() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        // These should not be matched
        create_fake_package(dir, "garden-moss");
        create_fake_package(dir, "readme.md");
        create_fake_package(dir, "something-linux-x64.tar.gz");
        // This should match
        create_fake_package(dir, "zen-garden-0.2.100-linux-x64.tar.gz");

        let suffix = "-linux-x64.tar.gz";
        let candidates: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("zen-garden-") && name.ends_with(suffix)
            })
            .collect();

        assert_eq!(candidates.len(), 1);
    }
}

#[cfg(test)]
mod deployment_tests {
    use std::fs;
    use std::path::Path;

    /// Helper: create a mock package directory structure
    fn create_mock_package(root: &Path) {
        let bin_dir = root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("garden-moss"), b"mock-binary").unwrap();
        fs::write(bin_dir.join("garden-rake"), b"mock-binary").unwrap();

        let companions_dir = bin_dir.join("companions").join("cricket");
        fs::create_dir_all(&companions_dir).unwrap();
        fs::write(companions_dir.join("garden-cricket"), b"mock-binary").unwrap();

        // package.json
        let manifest = serde_json::json!({
            "version": "0.2.999",
            "platform": "linux",
            "architecture": "x64",
            "components": {}
        });
        fs::write(
            root.join("package.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn mock_package_has_expected_structure() {
        let temp = tempfile::tempdir().unwrap();
        create_mock_package(temp.path());

        assert!(temp.path().join("bin/garden-moss").exists());
        assert!(temp.path().join("bin/garden-rake").exists());
        assert!(
            temp.path()
                .join("bin/companions/cricket/garden-cricket")
                .exists()
        );
        assert!(temp.path().join("package.json").exists());
    }

    #[test]
    fn find_bin_dir_direct_layout() {
        let temp = tempfile::tempdir().unwrap();
        create_mock_package(temp.path());

        let bin = temp.path().join("bin");
        assert!(bin.exists());
        assert!(bin.join("garden-moss").exists());
    }

    #[test]
    fn find_bin_dir_nested_layout() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("zen-garden-0.2.999-linux-x64");
        create_mock_package(&nested);

        // Search from the parent (staging dir) — should find nested/bin/
        let mut found = None;
        let direct = temp.path().join("bin");
        if direct.exists() {
            found = Some(direct);
        } else if let Ok(entries) = fs::read_dir(temp.path()) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let nested_bin = entry.path().join("bin");
                    if nested_bin.exists() {
                        found = Some(nested_bin);
                        break;
                    }
                }
            }
        }

        assert!(found.is_some(), "Should find bin/ in nested layout");
        assert!(found.unwrap().join("garden-moss").exists());
    }

    #[test]
    fn copy_dir_contents_mirrors_structure() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        // Create source structure
        let sub = src.path().join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(src.path().join("file1.txt"), b"hello").unwrap();
        fs::write(sub.join("file2.txt"), b"world").unwrap();

        // Copy
        copy_dir_recursive(src.path(), dest.path()).unwrap();

        assert!(dest.path().join("file1.txt").exists());
        assert!(dest.path().join("subdir/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(dest.path().join("file1.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(dest.path().join("subdir/file2.txt")).unwrap(),
            "world"
        );
    }

    fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
        if !src.is_dir() {
            return Ok(());
        }
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dest_path)?;
            } else {
                fs::copy(&src_path, &dest_path)?;
            }
        }
        Ok(())
    }

    #[test]
    fn install_options_default_no_yes_no_dry_run() {
        let options = crate::infra::installer::InstallOptions::default();
        assert!(!options.yes);
        assert!(!options.dry_run);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod legacy_migration_tests {
    use crate::infra::installer::pre_start::unit_file_needs_regeneration;

    #[test]
    fn old_shell_script_exec_start_pre_needs_regen() {
        let old = "[Service]\nType=simple\nExecStartPre=/usr/local/bin/moss-update-helper.sh\n";
        assert!(unit_file_needs_regeneration(old));
    }

    #[test]
    fn even_older_upgrade_script_needs_regen() {
        let old = "[Service]\nType=notify\nExecStartPre=/usr/local/bin/garden-upgrade.sh\n";
        assert!(unit_file_needs_regeneration(old));
    }

    #[test]
    fn sandbox_directives_need_regen() {
        let old = "[Service]\nType=notify\nProtectSystem=strict\nExecStartPre=/usr/local/bin/garden-moss pre-start\n";
        assert!(unit_file_needs_regeneration(old));
    }

    #[test]
    fn type_simple_needs_regen() {
        let old = "[Service]\nType=simple\nWatchdogSec=60\nNotifyAccess=main\n";
        assert!(unit_file_needs_regeneration(old));
    }

    #[test]
    fn missing_watchdog_needs_regen() {
        // Has NotifyAccess but no WatchdogSec
        let partial = "[Service]\nType=notify\nNotifyAccess=main\nExecStartPre=/usr/local/bin/garden-moss pre-start\n";
        assert!(unit_file_needs_regeneration(partial));
    }

    #[test]
    fn missing_notify_access_needs_regen() {
        // Has WatchdogSec but no NotifyAccess
        let partial = "[Service]\nType=notify\nWatchdogSec=60\nExecStartPre=/usr/local/bin/garden-moss pre-start\n";
        assert!(unit_file_needs_regeneration(partial));
    }

    #[test]
    fn modern_generated_unit_does_not_need_regen() {
        let modern = crate::infra::installer::linux::generate_unit_file();
        assert!(
            !unit_file_needs_regeneration(&modern),
            "The generated unit file itself must pass the staleness check"
        );
    }
}
