//! Linux network platform adapters
//!
//! Supports (in detection order):
//! - ifupdown (Debian, legacy Ubuntu) - `/etc/network/interfaces.d/`
//! - netplan (Ubuntu with systemd-networkd) - `/etc/netplan/`
//! - NetworkManager (various distros) - via nmcli
//!
//! ## ifupdown Approach (Debian)
//!
//! Creates `/etc/network/interfaces.d/99-zen-garden-static` with static IP config.
//! Uses `ifdown`/`ifup` to apply changes.
//!
//! ## Netplan Approach (Ubuntu)
//!
//! Creates `/etc/netplan/99-zen-garden-static.yaml` with high priority (99).
//! Uses `netplan apply` to apply changes.
//!
//! ## Safety
//!
//! - Never modify existing config files - only add/remove our own
//! - DHCP always available as fallback (remove our config)
//! - Config files clearly marked as managed by zen-garden

use super::{NetworkPlatform, StaticIpApply};
use crate::domain::NetworkError;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

/// ifupdown config directory
const IFUPDOWN_CONFIG_DIR: &str = "/etc/network/interfaces.d";
const IFUPDOWN_CONFIG_FILE: &str = "/etc/network/interfaces.d/99-zen-garden-static";
const IFUP_BINARY: &str = "/sbin/ifup";
const IFDOWN_BINARY: &str = "/sbin/ifdown";

/// Netplan config file path (high priority to override DHCP)
const NETPLAN_CONFIG_PATH: &str = "/etc/netplan/99-zen-garden-static.yaml";
const NETPLAN_BINARY: &str = "/usr/sbin/netplan";

/// NetworkManager nmcli binary path
const NMCLI_BINARY: &str = "/usr/bin/nmcli";

/// Detect which Linux network platform is available
///
/// Detection order (most reliable first):
/// 1. ifupdown - Standard Debian, most stable
/// 2. netplan - Ubuntu Server
/// 3. NetworkManager - Desktop distros
pub fn detect_linux_platform() -> Option<Box<dyn NetworkPlatform>> {
    // Check for ifupdown first (standard Debian)
    if std::path::Path::new(IFUP_BINARY).exists()
        && std::path::Path::new(IFDOWN_BINARY).exists()
        && std::path::Path::new(IFUPDOWN_CONFIG_DIR).exists()
    {
        tracing::debug!("Detected ifupdown network stack (Debian-style)");
        return Some(Box::new(LinuxIfupdown));
    }

    // Check for netplan (Ubuntu)
    if std::path::Path::new(NETPLAN_BINARY).exists() {
        tracing::debug!("Detected netplan network stack (Ubuntu-style)");
        return Some(Box::new(LinuxNetplan));
    }

    // Check for NetworkManager
    if std::path::Path::new(NMCLI_BINARY).exists() {
        tracing::debug!("Detected NetworkManager network stack");
        return Some(Box::new(LinuxNetwork));
    }

    tracing::warn!(
        "No supported Linux network stack detected. Checked: ifupdown ({}/{}), netplan ({}), nmcli ({})",
        IFUP_BINARY, IFDOWN_BINARY, NETPLAN_BINARY, NMCLI_BINARY
    );
    None
}

// ============================================================================
// ifupdown Adapter (Debian)
// ============================================================================

/// Linux ifupdown adapter
///
/// Creates `/etc/network/interfaces.d/99-zen-garden-static` with static IP config.
/// Uses ifdown/ifup to apply changes. Standard on Debian systems.
pub struct LinuxIfupdown;

impl NetworkPlatform for LinuxIfupdown {
    fn name(&self) -> &'static str {
        "ifupdown"
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(IFUP_BINARY).exists()
            && std::path::Path::new(IFDOWN_BINARY).exists()
            && std::path::Path::new(IFUPDOWN_CONFIG_DIR).exists()
    }

    fn apply_static<'a>(
        &'a self,
        config: &'a StaticIpApply,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            // Step 1: Write config file for persistence (on next boot)
            let cfg_content = generate_ifupdown_config(config);
            let temp_path = "/tmp/zen-garden-network-config.tmp";
            tokio::fs::write(temp_path, &cfg_content)
                .await
                .map_err(|e| {
                    NetworkError::ApplyFailed(format!("Failed to write temp config: {}", e))
                })?;

            let output = tokio::process::Command::new("sudo")
                .args(["cp", temp_path, IFUPDOWN_CONFIG_FILE])
                .output()
                .await
                .map_err(|e| {
                    NetworkError::ApplyFailed(format!("Failed to run sudo cp: {}", e))
                })?;

            let _ = tokio::fs::remove_file(temp_path).await;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NetworkError::ApplyFailed(format!(
                    "sudo cp to {} failed: {}",
                    IFUPDOWN_CONFIG_FILE, stderr
                )));
            }

            tracing::debug!(path = IFUPDOWN_CONFIG_FILE, "Wrote ifupdown config file");

            // Step 2: Add static IP as SECONDARY address (keeps DHCP as fallback)
            //
            // IMPORTANT: We do NOT stop dhcpcd or flush existing addresses.
            // This ensures the stone remains reachable via DHCP if the static IP
            // has a conflict or other issue. Both IPs work simultaneously.
            //
            // The config file (Step 1) ensures persistence across reboots.

            // Add static IP as secondary address
            let addr_cidr = format!("{}/{}", config.address, config.prefix_length);
            let output = tokio::process::Command::new("sudo")
                .args(["ip", "addr", "add", &addr_cidr, "dev", &config.interface])
                .output()
                .await
                .map_err(|e| {
                    NetworkError::ApplyFailed(format!("Failed to add address: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // RTNETLINK answers: File exists is OK - address already there
                if !stderr.contains("File exists") {
                    return Err(NetworkError::ApplyFailed(format!(
                        "ip addr add {} failed: {}",
                        addr_cidr, stderr
                    )));
                }
            }

            // Ensure interface is up
            let _ = tokio::process::Command::new("sudo")
                .args(["ip", "link", "set", &config.interface, "up"])
                .output()
                .await;

            // Step 3: Routes and DNS
            //
            // We do NOT modify routes or DNS at runtime - DHCP's settings remain active.
            // This keeps the stone fully reachable. The config file (Step 1) ensures
            // the full static configuration (including gateway/DNS) applies on reboot.
            //
            // For offerings like Pi-hole that need specific DNS behavior, they manage
            // their own DNS configuration after deployment.

            tracing::info!(
                interface = %config.interface,
                address = %config.address,
                dhcp_preserved = true,
                "Static IP added as secondary address (DHCP preserved as fallback)"
            );

            Ok(())
        })
    }

    fn apply_dhcp<'a>(
        &'a self,
        interface: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            // Step 1: Read our config file to get the static IP before removing it
            let config_path = PathBuf::from(IFUPDOWN_CONFIG_FILE);
            let mut static_ip_to_remove: Option<String> = None;

            if config_path.exists() {
                // Try to parse the static IP from our config file
                if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
                    // Look for "address X.X.X.X/Y" line
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("address ") {
                            static_ip_to_remove =
                                Some(trimmed.trim_start_matches("address ").to_string());
                            break;
                        }
                    }
                }

                // Remove the config file
                let output = tokio::process::Command::new("sudo")
                    .args(["rm", "-f", IFUPDOWN_CONFIG_FILE])
                    .output()
                    .await
                    .map_err(|e| {
                        NetworkError::ApplyFailed(format!("Failed to run sudo rm: {}", e))
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(NetworkError::ApplyFailed(format!(
                        "sudo rm {} failed: {}",
                        IFUPDOWN_CONFIG_FILE, stderr
                    )));
                }

                tracing::debug!(
                    path = IFUPDOWN_CONFIG_FILE,
                    "Removed static IP config file"
                );
            }

            // Step 2: Remove only the specific static IP we added (not the DHCP address)
            if let Some(addr) = static_ip_to_remove {
                let output = tokio::process::Command::new("sudo")
                    .args(["ip", "addr", "del", &addr, "dev", interface])
                    .output()
                    .await;

                match output {
                    Ok(o) if o.status.success() => {
                        tracing::debug!(address = %addr, interface = %interface, "Removed static IP");
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        // "Cannot assign requested address" means IP wasn't there - that's OK
                        if !stderr.contains("Cannot assign") {
                            tracing::warn!(address = %addr, error = %stderr, "Failed to remove static IP");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(address = %addr, error = %e, "Failed to run ip addr del");
                    }
                }
            }

            // DHCP is still running (we never stopped it), so no need to restart dhcpcd
            tracing::info!(interface = %interface, "Reverted to DHCP-only (static IP removed)");

            Ok(())
        })
    }
}

/// Generate ifupdown config file content
fn generate_ifupdown_config(config: &StaticIpApply) -> String {
    let dns_servers = config
        .dns
        .iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        r#"# Managed by zen-garden - do not edit manually
# This file configures static IP for offerings that require stable addressing.
# Removing this file and running 'sudo ifdown/ifup {interface}' will revert to DHCP.

auto {interface}
iface {interface} inet static
    address {address}/{prefix}
    gateway {gateway}
    dns-nameservers {dns}
"#,
        interface = config.interface,
        address = config.address,
        prefix = config.prefix_length,
        gateway = config.gateway,
        dns = dns_servers,
    )
}

// ============================================================================
// Netplan Adapter (Ubuntu)
// ============================================================================

/// Linux netplan adapter
///
/// Creates `/etc/netplan/99-zen-garden-static.yaml` with static IP config.
/// Uses high priority (99) to override any DHCP configuration.
pub struct LinuxNetplan;

impl NetworkPlatform for LinuxNetplan {
    fn name(&self) -> &'static str {
        "netplan"
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(NETPLAN_BINARY).exists()
    }

    fn apply_static<'a>(
        &'a self,
        config: &'a StaticIpApply,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            // Generate netplan YAML
            let yaml = generate_netplan_yaml(config);

            // Write to temp file first, then sudo cp to final location
            let temp_path = "/tmp/zen-garden-netplan-config.tmp";
            tokio::fs::write(temp_path, &yaml).await.map_err(|e| {
                NetworkError::ApplyFailed(format!("Failed to write temp netplan config: {}", e))
            })?;

            // Copy to final location via sudo
            let output = tokio::process::Command::new("sudo")
                .args(["cp", temp_path, NETPLAN_CONFIG_PATH])
                .output()
                .await
                .map_err(|e| {
                    NetworkError::ApplyFailed(format!("Failed to run sudo cp: {}", e))
                })?;

            // Clean up temp file
            let _ = tokio::fs::remove_file(temp_path).await;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NetworkError::ApplyFailed(format!(
                    "sudo cp to {} failed: {}",
                    NETPLAN_CONFIG_PATH, stderr
                )));
            }

            tracing::debug!(
                path = NETPLAN_CONFIG_PATH,
                "Wrote netplan config file via sudo cp"
            );

            // Apply configuration via sudo
            let output = tokio::process::Command::new("sudo")
                .args([NETPLAN_BINARY, "apply"])
                .output()
                .await
                .map_err(|e| {
                    NetworkError::ApplyFailed(format!("Failed to run sudo netplan apply: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NetworkError::ApplyFailed(format!(
                    "netplan apply failed: {}",
                    stderr
                )));
            }

            tracing::info!(
                interface = %config.interface,
                address = %config.address,
                "netplan configuration applied"
            );

            Ok(())
        })
    }

    fn apply_dhcp<'a>(
        &'a self,
        interface: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            let config_path = PathBuf::from(NETPLAN_CONFIG_PATH);

            // Remove our config file if it exists (via sudo since we run as non-root)
            if config_path.exists() {
                let output = tokio::process::Command::new("sudo")
                    .args(["rm", "-f", NETPLAN_CONFIG_PATH])
                    .output()
                    .await
                    .map_err(|e| {
                        NetworkError::ApplyFailed(format!("Failed to run sudo rm: {}", e))
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(NetworkError::ApplyFailed(format!(
                        "sudo rm {} failed: {}",
                        NETPLAN_CONFIG_PATH, stderr
                    )));
                }

                tracing::debug!(
                    path = NETPLAN_CONFIG_PATH,
                    "Removed netplan config file via sudo"
                );

                // Apply to revert to DHCP (underlying config should have DHCP)
                let output = tokio::process::Command::new("sudo")
                    .args([NETPLAN_BINARY, "apply"])
                    .output()
                    .await
                    .map_err(|e| {
                        NetworkError::ApplyFailed(format!(
                            "Failed to run sudo netplan apply: {}",
                            e
                        ))
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(NetworkError::ApplyFailed(format!(
                        "netplan apply failed: {}",
                        stderr
                    )));
                }
            }

            tracing::info!(interface = %interface, "Reverted to DHCP via netplan");

            Ok(())
        })
    }
}

/// Generate netplan YAML configuration
fn generate_netplan_yaml(config: &StaticIpApply) -> String {
    let dns_list = config
        .dns
        .iter()
        .map(|ip| format!("          - {}", ip))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"# Managed by zen-garden - do not edit manually
# This file configures static IP for offerings that require stable addressing.
# Removing this file and running 'netplan apply' will revert to DHCP.
network:
  version: 2
  renderer: networkd
  ethernets:
    {interface}:
      addresses:
        - {address}/{prefix}
      routes:
        - to: default
          via: {gateway}
      nameservers:
        addresses:
{dns}
"#,
        interface = config.interface,
        address = config.address,
        prefix = config.prefix_length,
        gateway = config.gateway,
        dns = dns_list,
    )
}

// ============================================================================
// NetworkManager Adapter (Stub for Phase 4)
// ============================================================================

/// Linux NetworkManager adapter
///
/// Uses nmcli to configure static IP.
/// This is a stub implementation for Phase 4.
pub struct LinuxNetwork;

impl NetworkPlatform for LinuxNetwork {
    fn name(&self) -> &'static str {
        "NetworkManager"
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(NMCLI_BINARY).exists()
    }

    fn apply_static<'a>(
        &'a self,
        config: &'a StaticIpApply,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            // Phase 4 implementation
            // nmcli connection modify "Wired connection 1" ipv4.method manual ipv4.addresses "192.168.1.100/24" ipv4.gateway "192.168.1.1" ipv4.dns "8.8.8.8"
            // nmcli connection up "Wired connection 1"

            tracing::warn!(
                interface = %config.interface,
                "NetworkManager static IP not yet implemented (Phase 4)"
            );

            Err(NetworkError::PlatformNotSupported(
                "NetworkManager support coming in Phase 4".to_string(),
            ))
        })
    }

    fn apply_dhcp<'a>(
        &'a self,
        interface: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), NetworkError>> + Send + 'a>> {
        Box::pin(async move {
            tracing::warn!(
                interface = %interface,
                "NetworkManager DHCP revert not yet implemented (Phase 4)"
            );

            Err(NetworkError::PlatformNotSupported(
                "NetworkManager support coming in Phase 4".to_string(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ifupdown_config() {
        let config = StaticIpApply {
            interface: "eth0".to_string(),
            address: "192.168.1.100".parse().unwrap(),
            prefix_length: 24,
            gateway: "192.168.1.1".parse().unwrap(),
            dns: vec!["8.8.8.8".parse().unwrap(), "1.1.1.1".parse().unwrap()],
        };

        let cfg = generate_ifupdown_config(&config);

        assert!(cfg.contains("Managed by zen-garden"));
        assert!(cfg.contains("iface eth0 inet static"));
        assert!(cfg.contains("address 192.168.1.100/24"));
        assert!(cfg.contains("gateway 192.168.1.1"));
        assert!(cfg.contains("dns-nameservers 8.8.8.8 1.1.1.1"));
    }

    #[test]
    fn test_generate_netplan_yaml() {
        let config = StaticIpApply {
            interface: "eth0".to_string(),
            address: "192.168.1.100".parse().unwrap(),
            prefix_length: 24,
            gateway: "192.168.1.1".parse().unwrap(),
            dns: vec!["8.8.8.8".parse().unwrap(), "1.1.1.1".parse().unwrap()],
        };

        let yaml = generate_netplan_yaml(&config);

        assert!(yaml.contains("Managed by zen-garden"));
        assert!(yaml.contains("eth0:"));
        assert!(yaml.contains("192.168.1.100/24"));
        assert!(yaml.contains("via: 192.168.1.1"));
        assert!(yaml.contains("- 8.8.8.8"));
        assert!(yaml.contains("- 1.1.1.1"));
    }

    #[test]
    fn test_platform_is_available() {
        // These tests depend on the environment
        let ifupdown = LinuxIfupdown;
        let _ = ifupdown.is_available();

        let netplan = LinuxNetplan;
        let _ = netplan.is_available();
    }
}
