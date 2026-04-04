//! Pre-start staged deployment (`garden-moss pre-start`)
//!
//! Replaces `moss-update-helper.sh` as the systemd `ExecStartPre` command.
//! Processes packages staged by `deploy.ps1` via the deploy API endpoint.
//!
//! This runs on every service start and must be fast when no staged
//! packages exist (the common case).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use garden_common::utils::validation::validate_safe_path;

use super::version::{InstallMethod, InstalledVersion};

/// Process any staged packages and exit.
///
/// Called as `ExecStartPre=/usr/local/bin/garden-moss pre-start`.
/// Always prints a version status line. When no staged packages exist,
/// prints the current version and exits quickly.
pub fn run(dry_run: bool) -> anyhow::Result<()> {
    // ── Legacy migration (runs every boot, fast no-op when clean) ───
    if !dry_run {
        migrate_legacy()?;
    }

    let staging_dir = validated_staging_dir();
    let version = current_version();

    if !staging_dir.join("bin").exists() {
        println!("[pre-start] {version} \u{2014} no staged upgrade.");
        if dry_run {
            println!("[pre-start] (dry-run)");
        }
        return Ok(());
    }

    println!("[pre-start] {version} \u{2014} found staged upgrade.");

    if dry_run {
        println!("[pre-start] DRY RUN \u{2014} listing actions without executing:");
        dry_run_report(&staging_dir)?;
        return Ok(());
    }

    deploy_bin(&staging_dir)?;
    deploy_scripts(&staging_dir)?;

    // Write version breadcrumb
    let staged_version = read_staged_version(&staging_dir);
    let breadcrumb = InstalledVersion::new(&staged_version, InstallMethod::PreStart);
    if let Err(e) = super::version::write_installed_version(&breadcrumb) {
        eprintln!("[pre-start] Warning: could not write version breadcrumb: {e}");
    }

    // Cleanup
    let validated = validated_staging_dir();
    if let Err(e) = std::fs::remove_dir_all(&validated) {
        eprintln!("[pre-start] Warning: could not clean staging: {e}");
    }

    println!("[pre-start] Upgrade complete: {version} -> v{staged_version}");
    Ok(())
}

// ── Legacy migration ────────────────────────────────────────────────

/// One-time self-healing migration from the old shell-script updater.
///
/// 1. Removes legacy shell scripts replaced by `garden-moss pre-start`.
/// 2. Regenerates the systemd unit file if it contains stale directives
///    (old ExecStartPre, Type=simple, ProtectSystem, missing WatchdogSec).
///
/// Each check is a file-exists or string-contains test — fast on every boot.
/// Once the artifacts are gone and the unit file is current, this is a no-op.
fn migrate_legacy() -> anyhow::Result<()> {
    let mut changed = false;

    // ── Remove legacy scripts ──────────────────────────────────────
    #[cfg(target_os = "linux")]
    for path_str in super::linux::LEGACY_SCRIPTS {
        let path = Path::new(path_str);
        if path.exists() {
            match std::fs::remove_file(path) {
                Ok(()) => {
                    println!("[pre-start] Removed legacy script: {path_str}");
                    changed = true;
                }
                Err(e) => {
                    eprintln!("[pre-start] Warning: could not remove {path_str}: {e}");
                }
            }
        }
    }

    // ── Regenerate unit file if stale ──────────────────────────────
    #[cfg(target_os = "linux")]
    {
        let unit_path = Path::new(super::linux::UNIT_FILE_PATH);
        if unit_path.exists() {
            if let Ok(current) = std::fs::read_to_string(unit_path) {
                if unit_file_needs_regeneration(&current) {
                    let new_contents = super::linux::generate_unit_file();
                    std::fs::write(unit_path, &new_contents).with_context(|| {
                        format!(
                            "failed to regenerate unit file at {}",
                            unit_path.display()
                        )
                    })?;
                    println!("[pre-start] Regenerated systemd unit file (legacy migration).");
                    changed = true;
                }
            }
        }
    }

    // ── daemon-reload if anything changed ──────────────────────────
    if changed {
        println!("[pre-start] Running systemctl daemon-reload...");
        let _ = Command::new("systemctl").args(["daemon-reload"]).output();
    }

    Ok(())
}

/// Returns true if the unit file contains legacy directives that indicate
/// it was generated before BUILD-0003 / ARCH-0008.
pub(crate) fn unit_file_needs_regeneration(contents: &str) -> bool {
    contents.contains("moss-update-helper.sh")
        || contents.contains("garden-upgrade.sh")
        || contents.contains("Type=simple")
        || contents.contains("ProtectSystem")
        || !contents.contains("WatchdogSec")
        || !contents.contains("NotifyAccess")
}

/// Current running binary version for display.
fn current_version() -> String {
    format!("v{}", crate::cli::VERSION)
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

        // Scripts landing in /usr/local/bin/ may be running executables —
        // use rename-then-copy to avoid ETXTBSY.
        let target_str = target.to_string_lossy();
        if target_str.starts_with("/usr/local/bin/") {
            replace_file(&file, &target)?;
        } else {
            std::fs::copy(&file, &target)?;
        }

        // Post-install hooks
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
            replace_file(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Replace a file at `dest` with `src`, handling running executables.
///
/// Uses rename-then-copy: atomically renames the existing file to `.old`,
/// copies the new file to the original path (fresh inode), then cleans up.
/// This avoids ETXTBSY (errno 26) because `rename(2)` detaches the directory
/// entry while the kernel keeps the old inode alive for any running process.
fn replace_file(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if dest.exists() {
        let backup = dest.with_extension("old");
        // Atomic rename: running process keeps the old inode via page mapping.
        // The path is now free for a fresh file.
        std::fs::rename(dest, &backup).with_context(|| {
            format!(
                "failed to rename {} -> {} before copy",
                dest.display(),
                backup.display()
            )
        })?;
        // Copy new file to original path (new inode — no ETXTBSY).
        let result = std::fs::copy(src, dest).with_context(|| {
            format!("failed to copy {} -> {}", src.display(), dest.display())
        });
        // Best-effort cleanup — old inode stays alive until the process exits.
        let _ = std::fs::remove_file(&backup);
        result?;
    } else {
        std::fs::copy(src, dest).with_context(|| {
            format!("failed to copy {} -> {}", src.display(), dest.display())
        })?;
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
