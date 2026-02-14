//! Windows-specific install and uninstall logic (Windows SCM)

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{FIREWALL_RULE_HTTP, FIREWALL_RULE_MDNS};
use super::{WINDOWS_DISPLAY_NAME, WINDOWS_SERVICE_DESCRIPTION, WINDOWS_SERVICE_NAME};

const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE_STOP_POLL: Duration = Duration::from_millis(500);

/// Install on Windows: deploy binaries, register service with SCM,
/// configure recovery policy and firewall rules.
pub fn install_platform(staging_dir: &Path, _work_dir: &Path) -> anyhow::Result<()> {
    let install_dir = super::platform_install_dir();
    std::fs::create_dir_all(&install_dir)?;

    // Create data directory
    let data_dir_str = garden_common::constants::paths::data_dir();
    let data_dir = Path::new(&data_dir_str);
    std::fs::create_dir_all(data_dir)?;
    println!("  Creating directories...");
    println!("    {}", install_dir.display());
    println!("    {}", data_dir.display());

    // Find and install binaries from package
    let bin_dir = find_bin_dir(staging_dir);
    install_binaries(&bin_dir, &install_dir)?;

    // Install companions subdirectory
    install_companions(&bin_dir)?;

    // Also copy the running binary if it's newer or different from the installed one
    let exe_path = std::env::current_exe()?;
    let installed_moss = install_dir.join("garden-moss.exe");
    if exe_path != installed_moss {
        // Copy ourselves to the install dir (the package may have a newer binary,
        // but the running binary is what the user invoked — trust the package binary
        // if present, otherwise copy ourselves)
        if !installed_moss.exists() {
            std::fs::copy(&exe_path, &installed_moss)?;
        }
    }

    // Register Windows service (idempotent upgrade)
    register_service(&install_dir)?;

    // Firewall rules (best-effort)
    setup_firewall(&installed_moss)?;

    Ok(())
}

fn find_bin_dir(staging_dir: &Path) -> PathBuf {
    let direct = staging_dir.join("bin");
    if direct.exists() {
        return direct;
    }

    if let Ok(entries) = std::fs::read_dir(staging_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.path().is_dir() {
                let nested = entry.path().join("bin");
                if nested.exists() {
                    return nested;
                }
            }
        }
    }

    staging_dir.to_path_buf()
}

fn install_binaries(bin_dir: &Path, install_dir: &Path) -> anyhow::Result<()> {
    println!("  Installing binaries...");

    if !bin_dir.exists() {
        return Ok(());
    }

    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let src = entry.path();
            if src.is_file() {
                let dest = install_dir.join(entry.file_name());
                std::fs::copy(&src, &dest)?;
                println!(
                    "    {} -> {}",
                    entry.file_name().to_string_lossy(),
                    dest.display()
                );
            }
        }
    }

    Ok(())
}

fn install_companions(bin_dir: &Path) -> anyhow::Result<()> {
    let companions_src = bin_dir.join("companions");
    if !companions_src.exists() {
        return Ok(());
    }

    let data_dir_str = garden_common::constants::paths::data_dir();
    let companions_dest = Path::new(&data_dir_str).join("Companions");
    std::fs::create_dir_all(&companions_dest)?;

    println!("  Installing companions...");

    if let Ok(entries) = std::fs::read_dir(&companions_src) {
        for entry in entries.filter_map(|e| e.ok()) {
            let src = entry.path();
            let dest = companions_dest.join(entry.file_name());
            if src.is_file() {
                std::fs::copy(&src, &dest)?;
                println!("    companions/{}", entry.file_name().to_string_lossy());
            }
        }
    }

    Ok(())
}

/// Register or upgrade the Windows service using sc.exe.
///
/// Follows Koi's idempotent pattern: stop -> delete -> wait -> recreate.
fn register_service(install_dir: &Path) -> anyhow::Result<()> {
    println!("  Registering Windows service...");

    let exe_path = install_dir.join("garden-moss.exe");
    let exe_path_str = exe_path.to_string_lossy();

    // Check if service already exists
    let check = Command::new("sc")
        .args(["query", WINDOWS_SERVICE_NAME])
        .output()?;

    if check.status.success() {
        // Upgrade path: stop, delete, wait for purge, recreate
        println!("    Existing service found, upgrading...");

        // Stop if running
        let status_output = Command::new("sc")
            .args(["query", WINDOWS_SERVICE_NAME])
            .output()?;
        let status_str = String::from_utf8_lossy(&status_output.stdout);
        if status_str.contains("RUNNING") {
            print!("    Stopping running service...");
            let _ = Command::new("sc")
                .args(["stop", WINDOWS_SERVICE_NAME])
                .output();
            wait_for_stop()?;
            println!(" done.");
        }

        // Delete old service
        let _ = Command::new("sc")
            .args(["delete", WINDOWS_SERVICE_NAME])
            .output();

        // Wait for SCM to purge the entry
        wait_for_delete()?;
    }

    // Create service
    let bin_path_arg = format!("binPath= \"{}\"", exe_path_str);
    let output = Command::new("sc")
        .args([
            "create",
            WINDOWS_SERVICE_NAME,
            &bin_path_arg,
            "start=",
            "auto",
            "DisplayName=",
            WINDOWS_DISPLAY_NAME,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "Failed to create service: {} {}",
            stdout.trim(),
            stderr.trim()
        );
    }

    println!("    Service: {} (auto-start)", WINDOWS_SERVICE_NAME);

    // Description (best-effort)
    let _ = Command::new("sc")
        .args([
            "description",
            WINDOWS_SERVICE_NAME,
            WINDOWS_SERVICE_DESCRIPTION,
        ])
        .output();

    // Recovery policy: restart after 5s, restart after 10s, then nothing
    // Reset failure count after 24 hours (86400 seconds)
    let _ = Command::new("sc")
        .args([
            "failure",
            WINDOWS_SERVICE_NAME,
            "reset=",
            "86400",
            "actions=",
            "restart/5000/restart/10000//",
        ])
        .output();

    // Trigger recovery on non-crash failures too
    let _ = Command::new("sc")
        .args(["failureflag", WINDOWS_SERVICE_NAME, "1"])
        .output();

    println!("    Recovery policy: restart 5s, 10s, then stop (resets after 24h)");

    Ok(())
}

fn setup_firewall(exe_path: &Path) -> anyhow::Result<()> {
    let http_port = garden_common::constants::MOSS_HTTP;
    let mdns_port = garden_common::constants::DISCOVERY_UDP;

    let fw_http = create_firewall_rule(FIREWALL_RULE_HTTP, "TCP", http_port, exe_path);
    let fw_mdns = create_firewall_rule(FIREWALL_RULE_MDNS, "UDP", mdns_port, exe_path);

    if fw_http && fw_mdns {
        println!(
            "  Firewall rules set (TCP {}, UDP {})",
            http_port, mdns_port
        );
    } else {
        if !fw_http {
            println!(
                "  Warning: could not set firewall rule for TCP {}",
                http_port
            );
        }
        if !fw_mdns {
            println!(
                "  Warning: could not set firewall rule for UDP {}",
                mdns_port
            );
        }
    }

    Ok(())
}

fn create_firewall_rule(name: &str, protocol: &str, port: u16, exe_path: &Path) -> bool {
    // Delete first for idempotency
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule"])
        .arg(format!("name={name}"))
        .output();

    let result = Command::new("netsh")
        .args(["advfirewall", "firewall", "add", "rule"])
        .arg(format!("name={name}"))
        .args(["dir=in", "action=allow"])
        .arg(format!("protocol={protocol}"))
        .arg(format!("localport={port}"))
        .arg(format!("program={}", exe_path.display()))
        .output();

    matches!(result, Ok(output) if output.status.success())
}

fn remove_firewall_rule(name: &str) -> bool {
    let result = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule"])
        .arg(format!("name={name}"))
        .output();

    matches!(result, Ok(output) if output.status.success())
}

// ── Service lifecycle helpers ────────────────────────────────────────

fn wait_for_stop() -> anyhow::Result<()> {
    let deadline = Instant::now() + SERVICE_STOP_TIMEOUT;
    loop {
        std::thread::sleep(SERVICE_STOP_POLL);

        let output = Command::new("sc")
            .args(["query", WINDOWS_SERVICE_NAME])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("STOPPED") {
            return Ok(());
        }

        if Instant::now() >= deadline {
            anyhow::bail!("Service did not stop within {:?}", SERVICE_STOP_TIMEOUT);
        }
    }
}

fn wait_for_delete() -> anyhow::Result<()> {
    let deadline = Instant::now() + SERVICE_STOP_TIMEOUT;
    loop {
        let output = Command::new("sc")
            .args(["query", WINDOWS_SERVICE_NAME])
            .output()?;

        if !output.status.success() {
            // Service no longer exists
            return Ok(());
        }

        if Instant::now() >= deadline {
            anyhow::bail!(
                "Old service entry not purged within {:?}",
                SERVICE_STOP_TIMEOUT
            );
        }

        std::thread::sleep(SERVICE_STOP_POLL);
    }
}

// ── Uninstall ────────────────────────────────────────────────────────

pub fn uninstall_platform() -> anyhow::Result<()> {
    // Stop and remove service
    let check = Command::new("sc")
        .args(["query", WINDOWS_SERVICE_NAME])
        .output()?;

    if check.status.success() {
        let status_str = String::from_utf8_lossy(&check.stdout);

        if status_str.contains("RUNNING") {
            print!("  Stopping service...");
            let _ = Command::new("sc")
                .args(["stop", WINDOWS_SERVICE_NAME])
                .output();
            wait_for_stop()?;
            println!(" done.");
        }

        print!("  Removing service...");
        let _ = Command::new("sc")
            .args(["delete", WINDOWS_SERVICE_NAME])
            .output();
        println!(" done.");
    } else {
        println!("  Service not found, cleaning up remaining files...");
    }

    // Remove firewall rules
    let rm_http = remove_firewall_rule(FIREWALL_RULE_HTTP);
    let rm_mdns = remove_firewall_rule(FIREWALL_RULE_MDNS);
    if rm_http || rm_mdns {
        println!("  Firewall rules removed.");
    }

    // Remove binaries from install directory
    let install_dir = super::platform_install_dir();
    if install_dir.exists() {
        println!("  Removing binaries...");
        if let Ok(entries) = std::fs::read_dir(&install_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    match std::fs::remove_file(&path) {
                        Ok(()) => println!("    {}", path.display()),
                        Err(e) => println!("    {} (warning: {e})", path.display()),
                    }
                }
            }
        }

        // Try to remove the install directory (only succeeds if empty)
        let _ = std::fs::remove_dir(&install_dir);
    }

    Ok(())
}
