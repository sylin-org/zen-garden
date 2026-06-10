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
    // Non-fatal: a read-only /etc can block unit regeneration, but that must never abort pre-start
    // (bailing here is what stranded ExecStartPre at a deleted script and bricked stones on restart).
    // Apply the staged upgrade regardless; migrate_legacy maintains a shim in that case.
    if !dry_run {
        if let Err(e) = migrate_legacy() {
            eprintln!("[pre-start] legacy migration warning (non-fatal): {e}");
        }
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

/// Self-healing migration onto the single canonical installer (`garden-moss pre-start`).
///
/// Order matters and is the whole point:
/// 1. **Regenerate the unit if stale** — but *non-fatally*. On an immutable/read-only `/etc` the
///    write fails with `EROFS`; we log and keep the legacy `ExecStartPre` rather than aborting.
/// 2. **Reconcile legacy scripts against the unit** — NEVER delete a script the unit still
///    references (that strands `ExecStartPre` at a missing file — the exact bricking this recovers
///    from). On a read-only-`/etc` stone the legacy `ExecStartPre` is kept and turned into a thin
///    shim → `garden-moss pre-start`. On a writable stone the unit was regenerated to drop it, so
///    it is removed.
///
/// Fast on every boot (string-contains + file-exists), a no-op once converged. Called from BOTH
/// `pre-start` and the main daemon bootstrap — the bootstrap call is essential because a stone whose
/// unit still has the legacy `ExecStartPre=moss-update-helper.sh` never runs `garden-moss pre-start`.
pub(crate) fn migrate_legacy() -> anyhow::Result<()> {
    // Systemd-only: legacy unit/script migration is meaningless where there's no systemd
    // (Android runs under the watchdog; Windows under the SCM). Skip to avoid futile work.
    if garden_common::host::profile().runtime.scheduler != garden_common::host::Scheduler::Systemd {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let mut changed = false;

        // ── 1. Regenerate the unit file if stale (non-fatal) ───────
        // On a read-only /etc the write fails (EROFS). Keep the legacy ExecStartPre and shim it
        // below instead of bailing. Read the unit first so the reconciliation knows what
        // ExecStartPre still references after the (attempted) regen.
        let mut unit_contents = String::new();
        let unit_path = Path::new(super::linux::UNIT_FILE_PATH);
        if let Ok(current) = std::fs::read_to_string(unit_path) {
            unit_contents = current;
            if unit_file_needs_regeneration(&unit_contents) {
                let new_contents = super::linux::generate_unit_file();
                match std::fs::write(unit_path, &new_contents) {
                    Ok(()) => {
                        println!("[pre-start] Regenerated systemd unit file (legacy migration).");
                        unit_contents = new_contents;
                        changed = true;
                    }
                    Err(e) => eprintln!(
                        "[pre-start] Unit not regenerated ({e}); keeping legacy ExecStartPre and shimming it."
                    ),
                }
            }
        }

        // ── 2. Reconcile legacy scripts against the unit ───────────
        for path_str in super::linux::LEGACY_SCRIPTS {
            let path = Path::new(path_str);
            let referenced = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|name| unit_contents.contains(name))
                .unwrap_or(false);
            if referenced {
                // Unit still calls this script (read-only /etc). Keep it as a canonical shim.
                ensure_legacy_shim(path);
            } else if path.exists() {
                match std::fs::remove_file(path) {
                    Ok(()) => {
                        println!("[pre-start] Removed legacy script: {path_str}");
                        changed = true;
                    }
                    Err(e) => eprintln!("[pre-start] Warning: could not remove {path_str}: {e}"),
                }
            }
        }

        if changed {
            println!("[pre-start] Running systemctl daemon-reload...");
            let _ = Command::new("systemctl").args(["daemon-reload"]).output();
        }
    }

    Ok(())
}

/// Maintain a legacy `ExecStartPre` script as a thin shim to `garden-moss pre-start`.
///
/// Used only on stones whose systemd unit is on a read-only `/etc` and therefore cannot be
/// regenerated to drop the legacy `ExecStartPre`. Deleting the referenced script would strand
/// `ExecStartPre` at a missing file (the brick); instead we make it delegate to the canonical
/// updater. Idempotent — a no-op once the file already delegates to `pre-start`.
#[cfg(target_os = "linux")]
fn ensure_legacy_shim(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let moss = garden_common::host::profile()
        .paths
        .bin_install
        .join("garden-moss");
    let shim = format!(
        "#!/bin/sh\n\
         # Canonical shim (DEPLOY-0001). This stone's systemd unit is on a read-only /etc and still\n\
         # references this path as ExecStartPre; the unit cannot be regenerated in place, so delegate\n\
         # to the canonical updater. `garden-moss pre-start` is idempotent and a no-op when nothing\n\
         # is staged.\n\
         exec {} pre-start \"$@\"\n",
        moss.display()
    );
    let already = std::fs::read_to_string(path)
        .map(|c| c.contains("pre-start"))
        .unwrap_or(false);
    if already {
        return;
    }
    if let Err(e) = std::fs::write(path, &shim) {
        eprintln!(
            "[pre-start] Warning: could not maintain shim {}: {e}",
            path.display()
        );
        return;
    }
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    println!(
        "[pre-start] Maintained ExecStartPre shim -> garden-moss pre-start: {}",
        path.display()
    );
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

/// Root of the rollback snapshot. DEPLOY-0001 keeps the binaries being replaced here so a bad
/// self-update can be rolled back — but OUTSIDE `bin_install` and the companions scan path, so the
/// companion launcher (or anything scanning the live tree) can never pick up a backup binary. Each
/// backup mirrors its absolute path under this root. Lives under `data_dir`.
fn rollback_root() -> PathBuf {
    Path::new(&garden_common::constants::paths::data_dir()).join("rollback")
}

/// The rollback slot for a live file: its absolute path's normal components mirrored under
/// [`rollback_root`] (cross-platform — drops the root/drive prefix).
fn rollback_path_for(dest: &Path) -> PathBuf {
    use std::path::Component;
    let mut rel = PathBuf::new();
    for comp in dest.components() {
        if let Component::Normal(c) = comp {
            rel.push(c);
        }
    }
    rollback_root().join(rel)
}

/// Mark the running binaries "good" (DEPLOY-0001): delete the rollback snapshot. Called once the
/// process has proven it survives startup — until then the supervisor (the Android watchdog) can
/// roll back from the snapshot; after, the upgrade is committed. On Linux this is just cleanup
/// (systemd doesn't roll back).
pub(crate) fn commit_upgrade() {
    let rollback = rollback_root();
    if rollback.exists() {
        if let Err(e) = std::fs::remove_dir_all(&rollback) {
            eprintln!(
                "[mark-good] Warning: could not remove rollback snapshot {}: {e}",
                rollback.display()
            );
        }
    }
}

/// Restore the rollback snapshot over the live binaries — the in-process analogue of the Android
/// watchdog's `mv "$MOSS_BACKUP" "$BIN"`, used by [`crash_loop_guard`]. The previous binaries are
/// mirrored under [`rollback_root`] by [`deploy_bin`]; re-prefix each with `/` to reconstruct its
/// absolute dest. Returns true if anything was restored.
///
/// The live moss binary is *running* (this is the crash-looping new binary), so a plain copy over it
/// fails with `ETXTBSY`. Rename the live file aside first — `rename` always succeeds (the running
/// process keeps its open inode) — then write the snapshot into the freed path; the next respawn
/// executes it.
#[cfg(target_os = "linux")]
pub(crate) fn restore_rollback() -> bool {
    use std::os::unix::fs::PermissionsExt;
    let root = rollback_root();
    if !root.exists() {
        return false;
    }
    let mut restored = false;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&root) else {
                continue;
            };
            let dest = Path::new("/").join(rel);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut aside = dest.clone().into_os_string();
            aside.push(".rollback-aside");
            let aside = PathBuf::from(aside);
            let _ = std::fs::rename(&dest, &aside); // move the live (bad) file aside; running inode survives
            if std::fs::copy(&path, &dest).is_ok() {
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
                let _ = std::fs::remove_file(&aside);
                restored = true;
                println!("[crash-loop] rolled back: {}", dest.display());
            } else {
                let _ = std::fs::rename(&aside, &dest); // copy failed — put the live file back
            }
        }
    }
    restored
}

/// Counter file for [`crash_loop_guard`] — boots since the current upgrade that have not reached
/// mark-good. Lives in `data_dir` (writable on every platform, unlike `/etc`).
#[cfg(target_os = "linux")]
fn boot_attempts_path() -> PathBuf {
    Path::new(&garden_common::constants::paths::data_dir()).join(".upgrade-boot-attempts")
}

/// Clear the boot-attempt counter. Called from mark-good — once moss has survived `MARK_GOOD_SECS`
/// the upgrade is good and the count is meaningless.
#[cfg(target_os = "linux")]
pub(crate) fn reset_boot_attempts() {
    let _ = std::fs::remove_file(boot_attempts_path());
}

/// DEPLOY-0001 Linux crash-loop rollback — the resilience the Android watchdog has, brought to
/// systemd without a unit change (so it covers read-only-`/etc` stones too). Called very early on
/// every boot:
///
/// - **No rollback snapshot** → no upgrade in flight → clear any stale counter and return.
/// - **Snapshot present** → count this boot. `MARK_GOOD_SECS` after a successful start, mark-good
///   resets the counter; so a count that keeps climbing means moss keeps crashing before it ever
///   becomes healthy. After [`MAX_UPGRADE_BOOTS`] such boots, restore the previous binaries and exit
///   `RESTART` — `Restart=always` respawns the rolled-back binary, which then marks good and commits.
#[cfg(target_os = "linux")]
pub(crate) fn crash_loop_guard() {
    /// Boots since an upgrade (without reaching mark-good) before we give up and roll back.
    const MAX_UPGRADE_BOOTS: u32 = 3;

    if !rollback_root().exists() {
        reset_boot_attempts();
        return;
    }

    let path = boot_attempts_path();
    let count = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
        + 1;

    if count >= MAX_UPGRADE_BOOTS {
        eprintln!(
            "[crash-loop] {count} boots since upgrade without reaching mark-good — rolling back"
        );
        let restored = restore_rollback();
        reset_boot_attempts();
        if restored {
            std::process::exit(garden_common::constants::server::exit::RESTART);
        }
        // Nothing actually restored (snapshot empty/unreadable) — let this boot proceed.
        return;
    }

    let _ = std::fs::write(&path, count.to_string());
}

// ── Binary deployment ───────────────────────────────────────────────

fn deploy_bin(staging_dir: &Path) -> anyhow::Result<()> {
    let bin_src = staging_dir.join("bin");
    if !bin_src.exists() {
        return Ok(());
    }

    // DEPLOY-0001: write to the host profile's paths, not a hardcoded /usr/local/bin (which is
    // read-only on Android). The companions/ subdir is routed to `paths.companions` — the dir the
    // runtime registry actually scans (companions.rs) — fixing the prior install≠scan mismatch.
    let profile = garden_common::host::profile();
    let bin_dest = profile.paths.bin_install.clone();
    let companions_dest = profile.paths.companions.clone();
    std::fs::create_dir_all(&bin_dest)?;

    for entry in std::fs::read_dir(&bin_src)? {
        let entry = entry?;
        let name = entry.file_name();
        let src_path = entry.path();
        if name.to_str() == Some("companions") {
            copy_dir_contents(&src_path, &companions_dest)?;
            set_executable_recursive(&companions_dest, 3)?;
        } else if src_path.is_dir() {
            copy_dir_contents(&src_path, &bin_dest.join(&name))?;
        } else {
            replace_file(&src_path, &bin_dest.join(&name))?;
        }
    }
    set_executable_recursive(&bin_dest, 1)?;

    let count = count_files(&bin_src)?;
    println!(
        "[pre-start] Deployed bin/ ({count} files) -> {} (companions -> {})",
        bin_dest.display(),
        companions_dest.display()
    );
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
/// Rename-aside in three moves: (1) atomically rename the existing file to a same-directory `.old`
/// (frees the path for a fresh inode without ETXTBSY — `rename(2)` detaches the directory entry
/// while the kernel keeps the old inode alive for any running process; on Windows you can rename a
/// locked `.exe` even though you can't overwrite it), (2) copy the new file into place, (3) stash
/// the `.old` into the rollback snapshot, OUT of the live tree.
///
/// Step 3 is the fix for the firefly/cricket "launched the `.old` backup" regression: the live tree
/// is left containing only the new binary, so the companion launcher (which scans that tree) can
/// never pick up a backup. The backup lives in [`rollback_root`]; mark-good deletes the snapshot
/// once the new binary proves healthy, and the supervisor restores from it on repeated failure.
fn replace_file(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if dest.exists() {
        let aside = dest.with_extension("old");
        // Clear any stale aside first (Unix rename overwrites; Windows rename fails if it exists).
        let _ = std::fs::remove_file(&aside);
        // Atomic rename: running process keeps the old inode via page mapping; the path is now free.
        std::fs::rename(dest, &aside).with_context(|| {
            format!(
                "failed to rename {} -> {} before copy",
                dest.display(),
                aside.display()
            )
        })?;
        // Copy new file to original path (new inode — no ETXTBSY).
        std::fs::copy(src, dest)
            .with_context(|| format!("failed to copy {} -> {}", src.display(), dest.display()))?;
        // Move the backup OUT of the live tree into the rollback snapshot.
        stash_backup(&aside, dest)?;
    } else {
        std::fs::copy(src, dest)
            .with_context(|| format!("failed to copy {} -> {}", src.display(), dest.display()))?;
    }
    Ok(())
}

/// Move the just-created `.old` aside-file out of the live tree into the rollback snapshot. Tries an
/// atomic rename first, then falls back to copy+remove across filesystems (the install paths and the
/// rollback root can be separate mounts — e.g. `/usr/local/bin` vs `/var/lib/zen-garden` on Linux).
fn stash_backup(aside: &Path, dest: &Path) -> anyhow::Result<()> {
    let backup = rollback_path_for(dest);
    if let Some(parent) = backup.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create rollback dir {}", parent.display()))?;
    }
    let _ = std::fs::remove_file(&backup); // clear any stale backup from a prior aborted apply
    if std::fs::rename(aside, &backup).is_ok() {
        return Ok(());
    }
    // Cross-filesystem fallback (reading the aside is fine — no process executes it via this path).
    std::fs::copy(aside, &backup).with_context(|| {
        format!(
            "failed to copy backup {} -> {}",
            aside.display(),
            backup.display()
        )
    })?;
    let _ = std::fs::remove_file(aside);
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
