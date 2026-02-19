//! Windows-specific install and uninstall logic (Windows SCM)

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{FIREWALL_RULE_PREFIX, LEGACY_FIREWALL_RULES};
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

/// Collect all firewall ports needed by Moss + embedded Koi capabilities.
fn all_firewall_ports() -> Vec<koi_common::firewall::FirewallPort> {
    use koi_common::firewall::{FirewallPort, FirewallProtocol};
    use std::collections::HashSet;

    // Build an equivalent KoiConfig to query its firewall_ports().
    // This mirrors the builder chain in bootstrap/run.rs.
    let koi_config = koi_embedded::KoiConfig {
        http_enabled: true,
        http_port: garden_common::constants::KOI_HTTP,
        mdns_enabled: true,
        dns_enabled: true,
        ..Default::default()
    };

    let mut ports = koi_config.firewall_ports();

    // Moss's own ports
    ports.push(FirewallPort::new(
        "Discovery",
        FirewallProtocol::Udp,
        garden_common::constants::DISCOVERY_UDP,
    ));
    ports.push(FirewallPort::new(
        "HTTP API",
        FirewallProtocol::Tcp,
        garden_common::constants::MOSS_HTTP,
    ));

    // Deduplicate by (protocol, port)
    let mut seen = HashSet::new();
    ports
        .into_iter()
        .filter(|p| seen.insert((p.protocol, p.port)))
        .collect()
}

fn setup_firewall(_exe_path: &Path) -> anyhow::Result<()> {
    // Clean up legacy rule names from pre-v0.2 installs
    for legacy in LEGACY_FIREWALL_RULES {
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule"])
            .arg(format!("name={legacy}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    let ports = all_firewall_ports();
    let count = koi_common::firewall::ensure_firewall_rules(FIREWALL_RULE_PREFIX, &ports);

    if count == ports.len() {
        let summary: Vec<_> = ports
            .iter()
            .map(|p| format!("{} {} ({})", p.protocol.as_str(), p.port, p.name))
            .collect();
        println!("  Firewall rules set ({})", summary.join(", "));
    } else {
        println!(
            "  Firewall: {}/{} rules set (some may need manual setup)",
            count,
            ports.len()
        );
    }

    Ok(())
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

    // Remove firewall rules (current names + legacy)
    let ports = all_firewall_ports();
    let mut removed = 0usize;
    for port in &ports {
        let rule_name = koi_common::firewall::firewall_rule_name(FIREWALL_RULE_PREFIX, port);
        let result = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule"])
            .arg(format!("name={rule_name}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if matches!(result, Ok(s) if s.success()) {
            removed += 1;
        }
    }
    for legacy in LEGACY_FIREWALL_RULES {
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule"])
            .arg(format!("name={legacy}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    if removed > 0 {
        println!("  Firewall rules removed ({removed} rules).");
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
