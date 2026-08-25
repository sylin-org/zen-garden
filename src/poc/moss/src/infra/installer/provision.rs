//! OS-level provisioning with interactive detection
//!
//! Two-phase design:
//! 1. **Probe**: detect what's missing without changing anything
//! 2. **Apply**: fix what's missing (each step is idempotent)
//!
//! The caller (mod.rs) runs the probe, shows results, prompts the user
//! (unless --yes), then calls apply with the list of needed fixes.

use std::process::Command;

/// A single environment component that can be probed and provisioned
#[derive(Debug, Clone)]
pub struct Component {
    pub name: &'static str,
    pub status: ComponentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentStatus {
    /// Present and working
    Ok,
    /// Missing, can be auto-installed
    Missing,
    /// Missing, requires manual action (with hint).
    /// Constructed on Windows only (e.g., Docker Desktop without winget).
    #[allow(dead_code)]
    Manual(String),
}

impl std::fmt::Display for ComponentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentStatus::Ok => write!(f, "ok"),
            ComponentStatus::Missing => write!(f, "not installed"),
            ComponentStatus::Manual(hint) => write!(f, "{hint}"),
        }
    }
}

// ── Probe phase ─────────────────────────────────────────────────────

/// Probe the system and return the status of each component.
/// Does not modify anything.
#[cfg(target_os = "linux")]
pub fn probe() -> Vec<Component> {
    vec![
        Component {
            name: "Docker",
            status: if command_exists("docker") {
                ComponentStatus::Ok
            } else {
                ComponentStatus::Missing
            },
        },
        Component {
            name: "stone user",
            status: {
                let user = garden_common::constants::paths::stone_user();
                if user_exists(&user) {
                    ComponentStatus::Ok
                } else {
                    ComponentStatus::Missing
                }
            },
        },
        Component {
            name: "avahi-daemon",
            status: if command_exists("avahi-daemon") {
                ComponentStatus::Ok
            } else {
                ComponentStatus::Missing
            },
        },
        Component {
            name: "DNS resolution",
            status: if command_exists("resolvectl") {
                ComponentStatus::Ok
            } else {
                ComponentStatus::Missing
            },
        },
        Component {
            name: "Directory ownership",
            status: {
                let user = garden_common::constants::paths::stone_user();
                let data = garden_common::constants::paths::data_dir();
                if user_exists(&user) && path_owned_by(&data, &user) {
                    ComponentStatus::Ok
                } else if !user_exists(&user) {
                    // Will be fixed after user creation
                    ComponentStatus::Missing
                } else {
                    ComponentStatus::Missing
                }
            },
        },
    ]
}

#[cfg(target_os = "windows")]
pub fn probe() -> Vec<Component> {
    vec![
        Component {
            name: "Docker Desktop",
            status: if command_exists("docker") {
                ComponentStatus::Ok
            } else if command_exists("winget") {
                ComponentStatus::Missing
            } else {
                ComponentStatus::Manual(
                    "not installed (install winget or download from docker.com)".to_string(),
                )
            },
        },
        Component {
            name: "Firewall rules",
            status: ComponentStatus::Missing, // Always re-apply on install
        },
    ]
}

/// Returns true if any components need provisioning
pub fn needs_provisioning(components: &[Component]) -> bool {
    components
        .iter()
        .any(|c| matches!(c.status, ComponentStatus::Missing))
}

/// Returns true if all components are ok (nothing to do)
pub fn all_ok(components: &[Component]) -> bool {
    components
        .iter()
        .all(|c| matches!(c.status, ComponentStatus::Ok))
}

// ── Apply phase ─────────────────────────────────────────────────────

/// Apply all missing provisioning steps (Linux).
/// Each step is idempotent — safe to run even if partially provisioned.
///
/// NOTE: Steps run sequentially. A hard failure (e.g., `chpasswd` not found)
/// aborts all subsequent steps, leaving the system partially provisioned.
/// Re-running `garden-moss install --yes` is safe and will resume from where
/// the previous run left off (each step checks before acting).
#[cfg(target_os = "linux")]
pub fn apply() -> anyhow::Result<()> {
    ensure_stone_user()?;
    ensure_directories()?;
    ensure_docker()?;
    ensure_avahi()?;
    configure_resolved()?;
    configure_ownership()?;
    configure_timezone_ntp()?;
    Ok(())
}

/// Apply all missing provisioning steps (Windows).
#[cfg(target_os = "windows")]
pub fn apply() -> anyhow::Result<()> {
    ensure_docker_windows()?;
    // Firewall rules are handled by windows.rs during install
    Ok(())
}

// ── Linux provisioning steps ────────────────────────────────────────

#[cfg(target_os = "linux")]
fn ensure_stone_user() -> anyhow::Result<()> {
    use std::path::Path;
    let user = garden_common::constants::paths::stone_user();

    if user_exists(&user) {
        return Ok(());
    }

    println!("  Creating user '{user}'...");
    run_cmd("useradd", &["-m", "-s", "/bin/bash", &user])?;

    // Set default password (same as username — changed by operator post-install)
    let input = format!("{user}:{user}");
    let mut child = Command::new("chpasswd")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(input.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("chpasswd failed for user '{user}' (exit code: {status})");
    }

    // Sudo group membership
    match Command::new("usermod")
        .args(["-aG", "sudo", &user])
        .output()
    {
        Ok(o) if !o.status.success() => {
            println!(
                "    Warning: could not add {user} to sudo group: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
        }
        Err(e) => println!("    Warning: could not add {user} to sudo group: {e}"),
        _ => {}
    }

    // Passwordless sudo
    let sudoers_path = format!("/etc/sudoers.d/{user}");
    if !Path::new(&sudoers_path).exists() {
        let rule = format!("{user} ALL=(ALL) NOPASSWD:ALL\n");
        std::fs::write(&sudoers_path, rule)?;
        set_permissions_mode(&sudoers_path, 0o440)?;
    }

    println!("    done.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_directories() -> anyhow::Result<()> {
    use std::path::Path;
    let home = garden_common::constants::paths::stone_home();
    let staging = garden_common::constants::paths::staging_dir();

    let dirs = [format!("{home}/bin"), staging, "/etc/netplan".to_string()];

    for dir in &dirs {
        if !Path::new(dir).exists() {
            std::fs::create_dir_all(dir)?;
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_docker() -> anyhow::Result<()> {
    if command_exists("docker") {
        return Ok(());
    }

    println!("  Installing Docker...");
    if command_exists("apt-get") {
        run_cmd("apt-get", &["update", "-qq"])?;
        run_cmd("apt-get", &["install", "-y", "-qq", "docker.io"])?;
    } else {
        println!("    Warning: apt-get not found, install Docker manually.");
        return Ok(());
    }

    // best-effort: Docker may already be running or systemd unavailable in container
    let _ = Command::new("systemctl")
        .args(["enable", "--now", "docker"])
        .output();

    // best-effort: user may already be in docker group
    let user = garden_common::constants::paths::stone_user();
    let _ = Command::new("usermod")
        .args(["-aG", "docker", &user])
        .output();

    println!("    done.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_avahi() -> anyhow::Result<()> {
    if command_exists("avahi-daemon") {
        return Ok(());
    }

    println!("  Installing avahi-daemon...");
    if command_exists("apt-get") {
        run_cmd("apt-get", &["install", "-y", "-qq", "avahi-daemon"])?;
    } else {
        println!("    Warning: apt-get not found, install avahi-daemon manually.");
        return Ok(());
    }

    // best-effort: avahi may already be running
    let _ = Command::new("systemctl")
        .args(["enable", "--now", "avahi-daemon"])
        .output();

    println!("    done.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_resolved() -> anyhow::Result<()> {
    use std::path::Path;
    // Install resolved if missing
    if !command_exists("resolvectl") {
        println!("  Installing systemd-resolved...");
        if command_exists("apt-get") {
            match Command::new("apt-get")
                .args(["install", "-y", "-qq", "systemd-resolved"])
                .output()
            {
                Ok(out) if !out.status.success() => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    println!("    Warning: apt-get failed: {}", stderr.trim());
                }
                Err(e) => println!("    Warning: could not run apt-get: {e}"),
                _ => {}
            }
        }
    }

    if !command_exists("resolvectl") {
        println!("    Warning: systemd-resolved not available, skipping DNS config.");
        return Ok(());
    }

    // Write mDNS resolve config
    let conf_dir = "/etc/systemd/resolved.conf.d";
    let conf_path = format!("{conf_dir}/zen-garden.conf");
    std::fs::create_dir_all(conf_dir)?;

    if !Path::new(&conf_path).exists() {
        std::fs::write(&conf_path, "[Resolve]\nMulticastDNS=resolve\n")?;
    }

    // best-effort: resolved may already be running
    let _ = Command::new("systemctl")
        .args(["enable", "--now", "systemd-resolved"])
        .output();

    // best-effort: networkd may already be running
    let _ = Command::new("systemctl")
        .args(["enable", "--now", "systemd-networkd"])
        .output();

    // best-effort: cosmetic — prevents 2-minute boot delay
    let _ = Command::new("systemctl")
        .args(["mask", "systemd-networkd-wait-online.service"])
        .output();

    // Symlink resolv.conf to resolved stub
    let resolv = "/etc/resolv.conf";
    let stub = "/run/systemd/resolve/stub-resolv.conf";
    let needs_symlink = match std::fs::read_link(resolv) {
        Ok(target) => target.to_string_lossy() != stub,
        Err(_) => true,
    };
    if needs_symlink {
        let _ = std::fs::remove_file(resolv);
        #[cfg(target_os = "linux")]
        std::os::unix::fs::symlink(stub, resolv)?;
    }

    println!("  Configuring DNS resolution... done.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_ownership() -> anyhow::Result<()> {
    let user = garden_common::constants::paths::stone_user();
    if !user_exists(&user) {
        return Ok(());
    }

    let data = garden_common::constants::paths::data_dir();
    let config = garden_common::constants::paths::config_dir();
    let home = garden_common::constants::paths::stone_home();

    let ownership = format!("{user}:{user}");

    for path in &[&data, &config] {
        match Command::new("chown").args([&ownership, path]).output() {
            Ok(o) if !o.status.success() => {
                println!(
                    "    Warning: chown {ownership} {path} failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                );
            }
            Err(e) => println!("    Warning: chown {ownership} {path} failed: {e}"),
            _ => {}
        }
    }

    match Command::new("chown")
        .args(["-R", &ownership, &home])
        .output()
    {
        Ok(o) if !o.status.success() => {
            println!(
                "    Warning: chown -R {ownership} {home} failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
        }
        Err(e) => println!("    Warning: chown -R {ownership} {home} failed: {e}"),
        _ => {}
    }

    println!("  Setting directory ownership... done.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_timezone_ntp() -> anyhow::Result<()> {
    let config_path = format!(
        "{}/garden-moss.toml",
        garden_common::constants::paths::config_dir()
    );
    if let Ok(contents) = std::fs::read_to_string(&config_path) {
        for line in contents.lines() {
            let trimmed = line.trim();
            if let Some(tz) = trimmed.strip_prefix("timezone") {
                let tz = tz.trim().trim_start_matches('=').trim().trim_matches('"');
                if !tz.is_empty() {
                    // best-effort: timezone is non-critical for operation
                    let _ = Command::new("timedatectl")
                        .args(["set-timezone", tz])
                        .output();
                }
            }
        }
    }

    // best-effort: NTP is non-critical for operation
    let _ = Command::new("timedatectl")
        .args(["set-ntp", "true"])
        .output();

    Ok(())
}

// ── Windows provisioning steps ──────────────────────────────────────

#[cfg(target_os = "windows")]
fn ensure_docker_windows() -> anyhow::Result<()> {
    if command_exists("docker") {
        return Ok(());
    }

    if !command_exists("winget") {
        println!("    winget not available. Install Docker Desktop manually from docker.com");
        return Ok(());
    }

    println!("  Installing Docker Desktop via winget...");
    let output = Command::new("winget")
        .args([
            "install",
            "Docker.DockerDesktop",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .output()?;

    if output.status.success() {
        println!("    done.");
        println!("    Note: A reboot may be required before Docker is available.");
        println!("          Moss will retry Docker connection on startup (up to 60s).");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("    Warning: winget install failed: {}", stderr.trim());
        println!("    Install Docker Desktop manually from docker.com");
    }

    Ok(())
}

// ── Shared helpers ──────────────────────────────────────────────────

fn command_exists(name: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        Command::new("command")
            .args(["-v", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("where")
            .arg(name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "linux")]
fn user_exists(name: &str) -> bool {
    Command::new("id")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn path_owned_by(path: &str, user: &str) -> bool {
    let output = Command::new("stat").args(["-c", "%U", path]).output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim() == user,
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn run_cmd(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{program} failed: {}", stderr.trim());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_permissions_mode(path: &str, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_status_display() {
        assert_eq!(ComponentStatus::Ok.to_string(), "ok");
        assert_eq!(ComponentStatus::Missing.to_string(), "not installed");
        assert_eq!(
            ComponentStatus::Manual("install from docker.com".to_string()).to_string(),
            "install from docker.com"
        );
    }

    #[test]
    fn needs_provisioning_true_when_missing() {
        let components = vec![
            Component {
                name: "Docker",
                status: ComponentStatus::Ok,
            },
            Component {
                name: "stone user",
                status: ComponentStatus::Missing,
            },
        ];
        assert!(needs_provisioning(&components));
    }

    #[test]
    fn needs_provisioning_false_when_all_ok() {
        let components = vec![
            Component {
                name: "Docker",
                status: ComponentStatus::Ok,
            },
            Component {
                name: "stone user",
                status: ComponentStatus::Ok,
            },
        ];
        assert!(!needs_provisioning(&components));
    }

    #[test]
    fn all_ok_true_when_all_ok() {
        let components = vec![Component {
            name: "Docker",
            status: ComponentStatus::Ok,
        }];
        assert!(all_ok(&components));
    }

    #[test]
    fn all_ok_false_when_manual() {
        let components = vec![Component {
            name: "Docker",
            status: ComponentStatus::Manual("install manually".to_string()),
        }];
        assert!(!all_ok(&components));
    }
}
