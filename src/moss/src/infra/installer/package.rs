//! Package resolution and extraction
//!
//! Resolves the platform package from:
//! 1. Local sibling file matching `zen-garden-*-{platform}.{ext}`
//! 2. GitHub Releases API (future)
//! 3. Graceful failure with clear instructions

use std::path::{Path, PathBuf};

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
/// 2. GitHub Releases API (not yet implemented)
/// 3. Graceful failure with download instructions
pub fn resolve_package(search_dir: &Path) -> anyhow::Result<PathBuf> {
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

    // 2. GitHub Releases (future implementation)
    // TODO: Implement GitHub release download
    //   - GET https://api.github.com/repos/{owner}/{repo}/releases/latest
    //   - Find asset matching zen-garden-*-{platform}.{ext}
    //   - Download with progress bar
    //   - Verify SHA256

    // 3. Graceful failure
    println!();
    anyhow::bail!(
        "No package found.\n\n\
         To install offline, place the platform package in the same directory as garden-moss:\n\
         \n\
         \x20 zen-garden-{{version}}-{platform}.{ext}\n\
         \n\
         Download from: https://github.com/koan-framework/zen-garden/releases/latest"
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

fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    use std::process::Command;

    // Use system tar for extraction
    let output = Command::new("tar")
        .args(["xzf", &archive_path.to_string_lossy()])
        .arg("-C")
        .arg(dest_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tar extraction failed: {}", stderr.trim());
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    use std::process::Command;

    // Use PowerShell Expand-Archive for extraction
    let script = format!(
        "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
        archive_path.display(),
        dest_dir.display()
    );

    let output = Command::new("powershell")
        .args(["-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("zip extraction failed: {}", stderr.trim());
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    use std::process::Command;

    let output = Command::new("unzip")
        .args(["-o", &archive_path.to_string_lossy()])
        .arg("-d")
        .arg(dest_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("zip extraction failed: {}", stderr.trim());
    }

    Ok(())
}
