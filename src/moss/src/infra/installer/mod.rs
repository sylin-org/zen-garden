//! Self-install and uninstall for Zen Garden Moss
//!
//! Provides `garden-moss install` and `garden-moss uninstall` commands.
//! These run synchronously in main() before the Tokio runtime — they must
//! never activate the daemon loop, API server, or service stack.
//!
//! Three installation tiers (all produce the same end state):
//! - **Online**: Binary downloads the latest package from GitHub Releases
//! - **Offline**: Binary + sibling package in the same directory
//! - **USB**: NewStone USB stick (unchanged, handled by preseed)

mod package;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use std::path::{Path, PathBuf};

use garden_common::infra::platform::is_running_from_removable_media;

/// Service name constants
#[cfg(target_os = "linux")]
const SERVICE_NAME: &str = "garden-moss";
#[cfg(target_os = "windows")]
const WINDOWS_SERVICE_NAME: &str = "ZenGardenMoss";
#[cfg(target_os = "windows")]
const WINDOWS_DISPLAY_NAME: &str = "Zen Garden Moss";
#[cfg(target_os = "windows")]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "Zen Garden stone orchestration daemon \u{2014} manages container services, \
     storage, and companions";

/// Firewall rule names (Windows)
#[cfg(target_os = "windows")]
const FIREWALL_RULE_HTTP: &str = "Zen Garden Moss HTTP (TCP)";
#[cfg(target_os = "windows")]
const FIREWALL_RULE_MDNS: &str = "Zen Garden Moss mDNS (UDP)";

/// Install Zen Garden as a system service.
///
/// Handles fresh installs and upgrades. Resolves a platform package
/// (local sibling or GitHub download), extracts it, installs binaries
/// and scripts, registers the service, and starts it.
///
/// If running from removable media (USB), copies the binary and package
/// to the permanent install location before proceeding.
pub fn install() -> anyhow::Result<()> {
    check_privileges("install")?;

    println!();
    println!("  Zen Garden Moss Installer");
    println!("  {}", crate::cli::VERSION);
    println!();

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine executable directory"))?;

    // If running from removable media, copy to permanent location first
    let (work_dir, copied_from_removable) = resolve_work_directory(&exe_path, exe_dir)?;

    // Phase 1: Resolve package
    println!("Resolving package...");
    let package_path = package::resolve_package(&work_dir)?;

    // Phase 2: Extract and install
    println!();
    println!("Installing Zen Garden...");

    let install_dir = platform_install_dir();
    std::fs::create_dir_all(&install_dir)?;

    // Extract package to staging, then install platform-specifically
    let staging_dir = staging_directory();
    std::fs::create_dir_all(&staging_dir)?;

    // Extract the package
    println!("  Extracting package...");
    package::extract_package(&package_path, &staging_dir)?;

    // Platform-specific installation
    #[cfg(target_os = "linux")]
    linux::install_platform(&staging_dir)?;

    #[cfg(target_os = "windows")]
    windows::install_platform(&staging_dir, &work_dir)?;

    // Cleanup staging
    let _ = std::fs::remove_dir_all(&staging_dir);

    // Cleanup removable media temp files
    if copied_from_removable {
        println!("  Cleaning up temporary files...");
        let _ = cleanup_removable_temp(&work_dir);
    }

    // Phase 3: Start and verify
    println!();
    start_and_verify()?;

    Ok(())
}

/// Uninstall Zen Garden service and binaries. Data is preserved.
pub fn uninstall() -> anyhow::Result<()> {
    check_privileges("uninstall")?;

    println!();
    println!("Uninstalling Zen Garden...");
    println!();

    #[cfg(target_os = "linux")]
    linux::uninstall_platform()?;

    #[cfg(target_os = "windows")]
    windows::uninstall_platform()?;

    // Print data preservation notice
    let data = garden_common::constants::paths::data_dir();
    let config = garden_common::constants::paths::config_dir();

    println!();
    println!("Zen Garden has been removed.");
    println!();
    println!("  Data preserved at:   {}", data);
    println!("  Config preserved at: {}", config);
    println!();

    #[cfg(target_os = "linux")]
    println!("  To remove all data: sudo rm -rf {} {}", data, config);
    #[cfg(target_os = "windows")]
    println!("  To remove all data, delete these directories manually.");

    println!();

    Ok(())
}

// ── Privilege checks ─────────────────────────────────────────────────

fn check_privileges(verb: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("id").arg("-u").output();
        match output {
            Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "0" => {}
            _ => anyhow::bail!(
                "garden-moss {verb} requires root \u{2014} try: sudo garden-moss {verb}"
            ),
        }
    }

    #[cfg(target_os = "windows")]
    {
        let ok = std::process::Command::new("net")
            .arg("session")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ok {
            anyhow::bail!(
                "garden-moss {verb} requires Administrator privileges \u{2014} \
                 right-click your terminal and choose \"Run as administrator\""
            );
        }
    }

    Ok(())
}

// ── Removable media handling ─────────────────────────────────────────

/// If running from removable media, copy the binary and sibling package
/// to a temporary directory on the system drive. Returns the work directory
/// and whether we copied from removable media.
fn resolve_work_directory(exe_path: &Path, exe_dir: &Path) -> anyhow::Result<(PathBuf, bool)> {
    let is_removable = is_running_from_removable_media(exe_path)?;

    if !is_removable {
        return Ok((exe_dir.to_path_buf(), false));
    }

    println!("  Detected removable media, copying to permanent location...");

    let temp_dir = install_temp_dir();
    std::fs::create_dir_all(&temp_dir)?;

    // Copy the binary
    let target_exe = temp_dir.join(exe_path.file_name().unwrap_or_default());
    std::fs::copy(exe_path, &target_exe)?;

    // Copy any sibling package files
    if let Ok(entries) = std::fs::read_dir(exe_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("zen-garden-") && (name.ends_with(".tar.gz") || name.ends_with(".zip")) {
                let dest = temp_dir.join(&name);
                std::fs::copy(entry.path(), &dest)?;
                println!("  Copied: {}", name);
            }
        }
    }

    Ok((temp_dir, true))
}

fn cleanup_removable_temp(temp_dir: &Path) -> anyhow::Result<()> {
    // Only clean up the install temp dir, not the entire permanent location
    let expected = install_temp_dir();
    if temp_dir == expected {
        std::fs::remove_dir_all(temp_dir)?;
    }
    Ok(())
}

// ── Platform paths ───────────────────────────────────────────────────

/// Where binaries are installed
fn platform_install_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/usr/local/bin")
    }
    #[cfg(target_os = "windows")]
    {
        let program_data = std::env::var("ProgramData")
            .unwrap_or_else(|_| r"C:\ProgramData".to_string());
        PathBuf::from(format!(r"{}\ZenGarden", program_data))
    }
}

/// Temporary directory for removable media copies and package extraction
fn install_temp_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/tmp/zen-garden-install")
    }
    #[cfg(target_os = "windows")]
    {
        let temp = std::env::var("TEMP")
            .unwrap_or_else(|_| r"C:\Windows\Temp".to_string());
        PathBuf::from(format!(r"{}\zen-garden-install", temp))
    }
}

/// Staging directory for package extraction during install
fn staging_directory() -> PathBuf {
    install_temp_dir().join("staging")
}

// ── Start and verify ─────────────────────────────────────────────────

fn start_and_verify() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        print!("Starting Moss...");
        let output = std::process::Command::new("systemctl")
            .args(["start", SERVICE_NAME])
            .output();
        match output {
            Ok(o) if o.status.success() => println!(" started."),
            Ok(o) => {
                println!();
                println!("  Warning: could not start service: {}",
                    String::from_utf8_lossy(&o.stderr).trim());
            }
            Err(e) => {
                println!();
                println!("  Warning: could not start service: {e}");
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        print!("Starting Moss...");
        let output = std::process::Command::new("sc")
            .args(["start", WINDOWS_SERVICE_NAME])
            .output();
        match output {
            Ok(o) if o.status.success() => println!(" started."),
            Ok(o) => {
                println!();
                let stderr = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                println!("  Warning: could not start service: {} {}", stdout.trim(), stderr.trim());
            }
            Err(e) => {
                println!();
                println!("  Warning: could not start service: {e}");
            }
        }
    }

    // Health check: poll for a few seconds
    println!();
    print!("Checking health...");
    let port = garden_common::constants::MOSS_HTTP;
    let health_url = format!("http://localhost:{}/health", port);
    let mut healthy = false;

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(response) = ureq_get(&health_url) {
            if response {
                healthy = true;
                break;
            }
        }
    }

    if healthy {
        println!(" healthy.");
    } else {
        println!(" not yet responding (this is normal during first boot).");
    }

    // Print success summary
    println!();
    println!("Zen Garden is ready.");
    println!();
    println!("  API:     http://localhost:{}", port);
    println!("  Health:  {}", health_url);
    println!("  CLI:     garden-rake status");
    println!();

    #[cfg(target_os = "linux")]
    {
        println!("  Manage the service:");
        println!("    systemctl status garden-moss      View status");
        println!("    systemctl stop garden-moss        Stop");
        println!("    systemctl restart garden-moss     Restart");
        println!("    journalctl -u garden-moss -f      Follow logs");
    }

    #[cfg(target_os = "windows")]
    {
        println!("  Manage the service:");
        println!("    sc query ZenGardenMoss            View status");
        println!("    sc stop ZenGardenMoss             Stop");
        println!("    sc start ZenGardenMoss            Start");
    }

    println!();

    Ok(())
}

/// Simple blocking HTTP GET that returns true if status is 2xx.
/// Uses raw TCP to avoid pulling in reqwest for synchronous context.
fn ureq_get(url: &str) -> anyhow::Result<bool> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    // Parse host:port from URL
    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{}", path);

    let mut stream = TcpStream::connect(host_port)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;

    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_port
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    // Check for 2xx status
    Ok(response.starts_with("HTTP/1.") && response.contains(" 200 "))
}
