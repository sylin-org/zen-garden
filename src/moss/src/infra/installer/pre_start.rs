//! Pre-start staged deployment (`garden-moss pre-start`)
//!
//! Replaces `moss-update-helper.sh` as the systemd `ExecStartPre` command.
//! Processes packages staged by `deploy.ps1` via the deploy API endpoint.
//!
//! This runs on every service start and must be fast when no staged
//! packages exist (the common case).

use std::path::{Path, PathBuf};
use std::process::Command;

use garden_common::utils::validation::validate_safe_path;

use super::version::{InstallMethod, InstalledVersion};

/// Process any staged packages and exit.
///
/// Called as `ExecStartPre=/usr/local/bin/garden-moss pre-start`.
/// If no staged packages exist, returns immediately.
pub fn run(dry_run: bool) -> anyhow::Result<()> {
    let staging_dir = validated_staging_dir();

    if !staging_dir.join("bin").exists() {
        if dry_run {
            println!("[pre-start] No staged packages found (dry-run).");
        }
        return Ok(());
    }

    println!(
        "[pre-start] Found staged upgrade in: {}",
        staging_dir.display()
    );

    if dry_run {
        println!("[pre-start] DRY RUN — listing actions without executing:");
        dry_run_report(&staging_dir)?;
        return Ok(());
    }

    deploy_bin(&staging_dir)?;
    deploy_scripts(&staging_dir)?;

    // Write version breadcrumb
    let version = read_staged_version(&staging_dir);
    let breadcrumb = InstalledVersion::new(&version, InstallMethod::PreStart);
    if let Err(e) = super::version::write_installed_version(&breadcrumb) {
        eprintln!("[pre-start] Warning: could not write version breadcrumb: {e}");
    }

    // Cleanup
    let validated = validated_staging_dir();
    if let Err(e) = std::fs::remove_dir_all(&validated) {
        eprintln!("[pre-start] Warning: could not clean staging: {e}");
    }

    println!("[pre-start] Upgrade complete.");
    Ok(())
}

// ── Binary deployment ───────────────────────────────────────────────

fn deploy_bin(staging_dir: &Path) -> anyhow::Result<()> {
    let bin_src = staging_dir.join("bin");
    if !bin_src.exists() {
        return Ok(());
    }

    let bin_dest = PathBuf::from("/usr/local/bin");

    // Copy all files from bin/ to /usr/local/bin/
    copy_dir_contents(&bin_src, &bin_dest)?;

    // Set executable permissions
    set_executable_recursive(&bin_dest, 1)?;

    // Handle companions subdirectory
    let companions = bin_dest.join("companions");
    if companions.exists() {
        set_executable_recursive(&companions, 3)?;
    }

    let count = count_files(&bin_src)?;
    println!("[pre-start] Deployed bin/ ({count} files) -> /usr/local/bin/");
    Ok(())
}

// ── Script deployment (filesystem-mirrored) ─────────────────────────

fn deploy_scripts(staging_dir: &Path) -> anyhow::Result<()> {
    let scripts_src = staging_dir.join("scripts");
    if !scripts_src.exists() {
        return Ok(());
    }

    println!("[pre-start] Deploying scripts/ (filesystem-mirrored)...");
    let mut needs_daemon_reload = false;

    for file in walk_files(&scripts_src)? {
        let rel = file.strip_prefix(&scripts_src)?;
        validate_safe_path(rel).map_err(|e| {
            anyhow::anyhow!("unsafe path in staged package: {}: {}", rel.display(), e)
        })?;
        let target = Path::new("/").join(rel);

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&file, &target)?;

        // Post-install hooks
        let target_str = target.to_string_lossy();
        if target_str.starts_with("/etc/systemd/system/") {
            needs_daemon_reload = true;
            set_mode(&target, 0o644)?;
        } else if target_str.starts_with("/usr/local/bin/") {
            set_mode(&target, 0o755)?;
        } else if target_str.starts_with("/var/lib/zen-garden/") {
            let user = garden_common::constants::paths::stone_user();
            match Command::new("chown")
                .args([&format!("{user}:{user}"), &target_str.to_string()])
                .output()
            {
                Ok(o) if !o.status.success() => {
                    eprintln!(
                        "[pre-start] Warning: chown failed for {}: {}",
                        target_str,
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[pre-start] Warning: chown failed for {}: {}",
                        target_str, e
                    );
                }
                _ => {}
            }
        }

        println!("[pre-start]   {} -> {}", rel.display(), target.display());
    }

    if needs_daemon_reload {
        println!("[pre-start] Running systemctl daemon-reload...");
        // best-effort: daemon-reload may fail in chroot/container environments
        let _ = Command::new("systemctl").args(["daemon-reload"]).output();
    }

    let count = count_files(&scripts_src)?;
    println!("[pre-start] Deployed scripts/ ({count} files).");
    Ok(())
}

// ── Dry run ─────────────────────────────────────────────────────────

fn dry_run_report(staging_dir: &Path) -> anyhow::Result<()> {
    let bin_src = staging_dir.join("bin");
    if bin_src.exists() {
        println!("  Would deploy bin/ -> /usr/local/bin/:");
        for file in walk_files(&bin_src)? {
            let rel = file.strip_prefix(&bin_src)?;
            println!("    {}", rel.display());
        }
    }

    let scripts_src = staging_dir.join("scripts");
    if scripts_src.exists() {
        println!("  Would deploy scripts/ (filesystem-mirrored):");
        for file in walk_files(&scripts_src)? {
            let rel = file.strip_prefix(&scripts_src)?;
            validate_safe_path(rel).map_err(|e| {
                anyhow::anyhow!("unsafe path in staged package: {}: {}", rel.display(), e)
            })?;
            let target = Path::new("/").join(rel);
            println!("    {} -> {}", rel.display(), target.display());
        }
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

fn validated_staging_dir() -> PathBuf {
    Path::new(&garden_common::constants::paths::staging_dir()).join("validated")
}

fn read_staged_version(staging_dir: &Path) -> String {
    // Try to read version from package.json in staging parent
    let package_json = staging_dir.parent().map(|p| p.join("package.json"));
    if let Some(path) = package_json {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(v) = value.get("version").and_then(|v| v.as_str()) {
                    return v.to_string();
                }
            }
        }
    }
    // Fallback: use the running binary's version
    crate::cli::VERSION.to_string()
}

fn copy_dir_contents(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_contents(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

fn set_executable_recursive(dir: &Path, max_depth: u32) -> anyhow::Result<()> {
    if max_depth == 0 || !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            set_mode(&path, 0o755)?;
        } else if path.is_dir() {
            set_executable_recursive(&path, max_depth - 1)?;
        }
    }
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    let _ = (path, mode); // suppress unused on non-linux
    Ok(())
}

fn walk_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_recursive(dir, &mut files)?;
    Ok(files)
}

fn walk_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn count_files(dir: &Path) -> anyhow::Result<usize> {
    Ok(walk_files(dir)?.len())
}
