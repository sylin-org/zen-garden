//! Package resolution and extraction
//!
//! Resolves the platform package from:
//! 1. Pre-staged packages (deploy API path)
//! 2. Local sibling file matching `zen-garden-*-{platform}.{ext}`
//! 3. GitHub Releases API (download latest matching asset)
//! 4. Graceful failure with clear instructions

use std::path::{Path, PathBuf};

/// GitHub repository for release downloads
const GITHUB_REPO: &str = "sylin-org/zen-garden";

/// Detect the current platform identifier for package matching
fn platform_id() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86"))]
    {
        "linux-x86"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

/// Package file extension for the current platform
fn platform_ext() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "tar.gz"
    }
    #[cfg(target_os = "windows")]
    {
        "zip"
    }
}

/// Resolve the platform package from local directory or GitHub.
///
/// Resolution order:
/// 1. Local sibling file matching `zen-garden-*-{platform}.{ext}`
/// 2. GitHub Releases latest matching asset (prompts unless auto_accept)
/// 3. Graceful failure with download instructions
pub fn resolve_package(search_dir: &Path, auto_accept: bool) -> anyhow::Result<PathBuf> {
    let platform = platform_id();
    let ext = platform_ext();

    // 1. Check for local sibling package
    println!("  Checking for zen-garden-*-{}.{}...", platform, ext);

    if let Some(local) = find_local_package(search_dir, platform, ext) {
        println!(
            "  Found: {}",
            local.file_name().unwrap_or_default().to_string_lossy()
        );
        return Ok(local);
    }

    println!("  No local package found.");

    // 2. Try GitHub Releases (prompt first unless --yes)
    let should_download = if auto_accept {
        true
    } else {
        print!("  Download latest from GitHub? [Y/n] ");
        super::prompt_yes_no(true)
    };

    if should_download {
        match download_from_github(search_dir, platform, ext) {
            Ok(path) => return Ok(path),
            Err(e) => {
                println!("  Could not download from GitHub: {e}");
            }
        }
    }

    // 3. Graceful failure
    println!();
    anyhow::bail!(
        "No package found.\n\n\
         To install offline, place the platform package in the same directory as garden-moss:\n\
         \n\
         \x20 zen-garden-{{version}}-{platform}.{ext}\n\
         \n\
         Download from: https://github.com/{GITHUB_REPO}/releases/latest"
    );
}

/// Find a local package file matching the platform pattern
fn find_local_package(dir: &Path, platform: &str, ext: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;

    let suffix = format!("-{}.{}", platform, ext);

    let mut candidates: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("zen-garden-") && name.ends_with(&suffix)
        })
        .map(|e| e.path())
        .collect();

    // Sort by name descending to get the latest version
    candidates.sort();
    candidates.pop()
}

// ── GitHub Releases download ────────────────────────────────────────

/// Download the latest matching package from GitHub Releases.
///
/// Uses raw TCP/HTTP to avoid pulling in reqwest for synchronous context.
/// Falls back gracefully if network is unavailable.
fn download_from_github(dest_dir: &Path, platform: &str, ext: &str) -> anyhow::Result<PathBuf> {
    let api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    // Fetch release metadata
    let release_json = https_get_string(&api_url)?;
    let release: serde_json::Value = serde_json::from_str(&release_json)?;

    let tag = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Find matching asset
    let suffix = format!("-{}.{}", platform, ext);
    let assets = release
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("No assets in release"))?;

    let asset = assets
        .iter()
        .find(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.starts_with("zen-garden-") && n.ends_with(&suffix))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow::anyhow!("No matching asset for platform '{platform}' in release {tag}")
        })?;

    let asset_name = asset
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("package");
    let download_url = asset
        .get("browser_download_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No download URL for asset"))?;
    let asset_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

    println!(
        "  Downloading {} ({})...",
        asset_name,
        format_bytes(asset_size)
    );

    // Download the asset
    let dest_path = dest_dir.join(asset_name);
    https_download(download_url, &dest_path)?;

    // Verify file size
    let downloaded_size = std::fs::metadata(&dest_path)?.len();
    if asset_size > 0 && downloaded_size != asset_size {
        let _ = std::fs::remove_file(&dest_path);
        anyhow::bail!(
            "Download size mismatch: expected {} bytes, got {}",
            asset_size,
            downloaded_size
        );
    }

    println!("  Downloaded: {}", asset_name);
    Ok(dest_path)
}

/// HTTPS GET returning response body as string.
/// Uses system curl for TLS support without pulling in native-tls/rustls.
fn https_get_string(url: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: garden-moss-installer",
            url,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("HTTP request failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// HTTPS download to file using system curl.
fn https_download(url: &str, dest: &Path) -> anyhow::Result<()> {
    let output = std::process::Command::new("curl")
        .args([
            "-fSL",
            "--max-time",
            "300",
            "--progress-bar",
            "-H",
            "User-Agent: garden-moss-installer",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Download failed: {}", stderr.trim());
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "unknown size".to_string();
    }
    garden_common::format_bytes(bytes)
}

// ── Extraction ──────────────────────────────────────────────────────

/// Extract a package to the staging directory
pub fn extract_package(package_path: &Path, staging_dir: &Path) -> anyhow::Result<()> {
    let name = package_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    if name.ends_with(".tar.gz") {
        extract_tar_gz(package_path, staging_dir)
    } else if name.ends_with(".zip") {
        extract_zip(package_path, staging_dir)
    } else {
        anyhow::bail!("Unsupported package format: {}", name);
    }
}

/// Extract a `.tar.gz` archive with per-entry path validation (BUILD-0004).
///
/// Each entry's path is validated BEFORE it is written to disk. Entries
/// containing `..`, absolute paths, backslashes, or symlinks are rejected
/// immediately, aborting extraction before any unsafe file is written.
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    use garden_common::utils::validation::validate_safe_path;
    use tar::EntryType;

    let file = std::fs::File::open(archive_path)
        .map_err(|e| anyhow::anyhow!("cannot open archive {}: {}", archive_path.display(), e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // Disable setting ownership from archive metadata
    archive.set_preserve_permissions(false);

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let entry_type = entry.header().entry_type();
        let entry_path = entry.path()?.into_owned();

        // Reject symlinks and hard links — zip-slip via symlink chains (BUILD-0004)
        match entry_type {
            EntryType::Regular | EntryType::Directory | EntryType::GNUSparse => {}
            EntryType::Symlink | EntryType::Link => {
                anyhow::bail!(
                    "archive contains a symlink or hard link (rejected): {}",
                    entry_path.display()
                );
            }
            other => {
                anyhow::bail!(
                    "archive contains unsupported entry type {:?}: {}",
                    other,
                    entry_path.display()
                );
            }
        }

        // Validate path BEFORE writing (BUILD-0004 invariant 1)
        validate_safe_path(&entry_path).map_err(|e| {
            anyhow::anyhow!("unsafe path in archive: {}: {}", entry_path.display(), e)
        })?;

        // Unpack into dest_dir — tar crate resolves relative to dest_dir
        entry.unpack_in(dest_dir)?;
    }

    Ok(())
}

/// Extract a `.zip` archive using platform tools, with post-extraction validation.
///
/// Uses PowerShell `Expand-Archive` on Windows and `unzip` on Linux.
/// Unlike tar.gz (which uses in-process extraction), zip uses shell commands
/// because the `zip` crate is not included. Post-extraction path validation
/// provides defense-in-depth.
///
/// ## Security (BUILD-0004)
///
/// Paths are passed as separate `-LiteralPath` / `-DestinationPath` arguments —
/// never interpolated into a PowerShell command string — to prevent command
/// injection via paths containing embedded quotes or special characters.
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        // Pass paths as arguments, not via string interpolation, to prevent
        // command injection via paths containing embedded quotes (BUILD-0004).
        let output = Command::new("powershell")
            .args([
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "Expand-Archive",
                "-LiteralPath",
            ])
            .arg(archive_path)
            .arg("-DestinationPath")
            .arg(dest_dir)
            .arg("-Force")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("zip extraction failed: {}", stderr.trim());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let output = Command::new("unzip")
            .args(["-o", &archive_path.to_string_lossy()])
            .arg("-d")
            .arg(dest_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("zip extraction failed: {}", stderr.trim());
        }
    }

    // Post-extraction path validation (BUILD-0004)
    validate_extracted_paths(dest_dir)?;

    Ok(())
}

/// Walk all extracted files and reject paths containing parent-directory traversal.
///
/// Used as defense-in-depth for zip extraction (which uses shell commands).
/// Tar.gz extraction validates per-entry before writing, so this is only
/// needed for the zip path.
fn validate_extracted_paths(dir: &Path) -> anyhow::Result<()> {
    use garden_common::utils::validation::validate_safe_path;

    for entry in walkdir_all(dir)? {
        let rel = entry
            .strip_prefix(dir)
            .map_err(|_| anyhow::anyhow!("extracted path outside staging: {}", entry.display()))?;
        validate_safe_path(rel).map_err(|e| {
            anyhow::anyhow!("unsafe path in extracted archive: {}: {}", rel.display(), e)
        })?;
    }
    Ok(())
}

fn walkdir_all(dir: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    walkdir_all_recursive(dir, &mut files)?;
    Ok(files)
}

fn walkdir_all_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walkdir_all_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}
