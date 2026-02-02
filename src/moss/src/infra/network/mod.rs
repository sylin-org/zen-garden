//! Network infrastructure layer
//!
//! Platform-specific network configuration implementation:
//! - IP conflict probing (ARP/ICMP)
//! - Platform adapters (netplan, NetworkManager, Windows)
//! - State persistence
//!
//! ## Design
//!
//! This module follows SoC principles:
//! - Domain layer defines what (NetworkMode, StaticIpState)
//! - Infrastructure layer defines how (platform-specific implementation)
//!
//! ## Safety
//!
//! All operations are designed to be fail-safe:
//! - DHCP fallback on any failure
//! - Never modify existing network configs (add new files)
//! - Atomic operations with rollback

pub mod probe;
pub mod state;

#[cfg(target_os = "linux")]
pub mod linux;

use crate::domain::{NetworkError, ProbeResult, StaticIpState};
use crate::infra::StaticIpPoolConfig;
use std::net::Ipv4Addr;

// Re-exports
pub use probe::{probe_ip_conflict, ProbeConfig};
pub use state::{load_network_state, save_network_state};

#[cfg(target_os = "linux")]
pub use linux::{detect_linux_platform, LinuxNetplan, LinuxNetworkManager};

/// Platform-specific network configuration trait
///
/// Implementations handle the actual network configuration for each platform.
/// Domain layer calls these methods without knowing platform specifics.
#[async_trait::async_trait]
pub trait NetworkPlatform: Send + Sync {
    /// Platform name for logging
    fn name(&self) -> &'static str;

    /// Apply static IP configuration
    async fn apply_static(&self, config: &StaticIpApply) -> Result<(), NetworkError>;

    /// Revert to DHCP
    async fn apply_dhcp(&self, interface: &str) -> Result<(), NetworkError>;

    /// Check if platform is available and properly configured
    fn is_available(&self) -> bool;
}

/// Static IP configuration to apply
#[derive(Debug, Clone)]
pub struct StaticIpApply {
    pub interface: String,
    pub address: Ipv4Addr,
    pub prefix_length: u8,
    pub gateway: Ipv4Addr,
    pub dns: Vec<Ipv4Addr>,
}

/// Detect available network platform
///
/// Returns the first available platform adapter, or None if no platform is supported.
pub fn detect_platform() -> Option<Box<dyn NetworkPlatform>> {
    #[cfg(target_os = "linux")]
    {
        if let Some(platform) = detect_linux_platform() {
            return Some(platform);
        }
    }

    // Windows and macOS support planned for Phase 4
    #[cfg(target_os = "windows")]
    {
        tracing::debug!("Windows static IP support not yet implemented");
    }

    #[cfg(target_os = "macos")]
    {
        tracing::debug!("macOS static IP support not yet implemented");
    }

    None
}

/// Batch size for parallel IP probing
/// 4 concurrent probes balances speed vs network load
const PROBE_BATCH_SIZE: usize = 4;

/// Select an available IP from the pool
///
/// Probes IPs in parallel batches for faster discovery.
/// Returns the lowest available IP from the first batch with availability.
/// Returns PoolExhausted error if all IPs have conflicts.
pub async fn select_ip_from_pool(
    config: &StaticIpPoolConfig,
    interface: &str,
) -> Result<Ipv4Addr, NetworkError> {
    use crate::domain::PoolExhausted;

    let probe_config = ProbeConfig::default();
    let mut conflicts = Vec::new();

    // Collect all IPs in pool
    let all_ips: Vec<Ipv4Addr> = config.iter().collect();
    let pool_size = all_ips.len();

    tracing::debug!(
        pool_start = %config.pool_start,
        pool_end = %config.pool_end,
        pool_size = pool_size,
        batch_size = PROBE_BATCH_SIZE,
        "Starting parallel IP probe"
    );

    // Process in batches
    for (batch_idx, batch) in all_ips.chunks(PROBE_BATCH_SIZE).enumerate() {
        tracing::debug!(
            batch = batch_idx,
            ips = ?batch,
            "Probing batch"
        );

        // Spawn probes for this batch concurrently
        let probe_futures: Vec<_> = batch
            .iter()
            .map(|&ip| {
                let interface = interface.to_string();
                let config = probe_config.clone();
                async move {
                    let result = probe_ip_conflict(ip, &interface, &config).await;
                    (ip, result)
                }
            })
            .collect();

        // Wait for all probes in batch to complete
        let results = futures_util::future::join_all(probe_futures).await;

        // Check results - prefer lowest IP if multiple available
        let mut batch_conflicts = Vec::new();
        let mut available_ip: Option<Ipv4Addr> = None;

        for (ip, result) in results {
            match result {
                ProbeResult::Available => {
                    tracing::debug!(ip = %ip, "IP available");
                    // Keep lowest available IP
                    if available_ip.is_none() || ip < available_ip.unwrap() {
                        available_ip = Some(ip);
                    }
                }
                ProbeResult::Conflict { method, responder_mac } => {
                    let reason = if let Some(mac) = responder_mac {
                        format!("{} conflict ({})", method, mac)
                    } else {
                        format!("{} conflict", method)
                    };
                    tracing::debug!(ip = %ip, reason = %reason, "IP has conflict");
                    batch_conflicts.push((ip, reason));
                }
                ProbeResult::LocalConflict => {
                    tracing::debug!(ip = %ip, "IP bound locally");
                    batch_conflicts.push((ip, "locally bound".to_string()));
                }
                ProbeResult::Error(e) => {
                    tracing::warn!(ip = %ip, error = %e, "Probe failed, treating as conflict");
                    batch_conflicts.push((ip, format!("probe error: {}", e)));
                }
            }
        }

        // If we found an available IP in this batch, use it
        if let Some(ip) = available_ip {
            tracing::info!(ip = %ip, batch = batch_idx, "Selected available IP from pool");
            return Ok(ip);
        }

        // No available IPs in this batch, record conflicts and continue
        conflicts.extend(batch_conflicts);
    }

    Err(NetworkError::PoolExhausted(PoolExhausted {
        pool_start: config.pool_start,
        pool_end: config.pool_end,
        conflicts,
    }))
}

/// Apply static IP from pool with conflict detection
///
/// High-level function that:
/// 1. Selects an available IP from the pool
/// 2. Applies it using the platform adapter
/// 3. Updates persistent state
///
/// Returns the applied IP address on success.
pub async fn apply_static_from_pool(
    config: &StaticIpPoolConfig,
    offering: &str,
    state: &mut StaticIpState,
) -> Result<Ipv4Addr, NetworkError> {
    use crate::domain::{NetworkMode, StaticIpActive, StaticIpDesired};
    use chrono::Utc;

    // Detect platform
    let platform = detect_platform().ok_or_else(|| {
        #[cfg(target_os = "linux")]
        {
            NetworkError::PlatformNotSupported(
                "No supported network manager found. Install one of: ifupdown (Debian), netplan (Ubuntu), or NetworkManager".to_string()
            )
        }
        #[cfg(target_os = "windows")]
        {
            NetworkError::PlatformNotSupported(
                "Windows static IP configuration not yet implemented".to_string()
            )
        }
        #[cfg(target_os = "macos")]
        {
            NetworkError::PlatformNotSupported(
                "macOS static IP configuration not yet implemented".to_string()
            )
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            NetworkError::PlatformNotSupported(
                format!("Unsupported operating system: {}", std::env::consts::OS)
            )
        }
    })?;

    // Determine interface
    let interface = config.interface.clone().unwrap_or_else(|| {
        // Auto-detect primary interface
        detect_primary_interface().unwrap_or_else(|| "eth0".to_string())
    });

    // Select available IP from pool
    let ip = select_ip_from_pool(config, &interface).await?;

    // Build apply config
    let apply_config = StaticIpApply {
        interface: interface.clone(),
        address: ip,
        prefix_length: config.prefix_length,
        gateway: config.gateway,
        dns: config.dns.clone(),
    };

    // Apply via platform adapter
    platform.apply_static(&apply_config).await?;

    // Update state
    state.add_requester(offering);
    state.mode = NetworkMode::static_ip(ip);
    state.desired = Some(StaticIpDesired {
        address: ip,
        prefix_length: config.prefix_length,
        gateway: config.gateway,
        dns: config.dns.clone(),
        interface: interface.clone(),
    });
    state.active = Some(StaticIpActive {
        address: ip,
        obtained_via: platform.name().to_string(),
        applied_at: Utc::now(),
    });

    // Persist state
    save_network_state(state).await?;

    tracing::info!(
        ip = %ip,
        interface = %interface,
        platform = platform.name(),
        offering = offering,
        "Static IP applied successfully"
    );

    Ok(ip)
}

/// Revert to DHCP when no offerings need static IP
pub async fn revert_to_dhcp(
    offering: &str,
    state: &mut StaticIpState,
) -> Result<(), NetworkError> {
    use crate::domain::NetworkMode;

    // Remove this offering from requesters
    let should_revert = state.remove_requester(offering);

    if !should_revert {
        tracing::debug!(
            offering = offering,
            remaining = state.requester_count(),
            "Offering removed but other requesters remain"
        );
        save_network_state(state).await?;
        return Ok(());
    }

    // No more requesters - revert to DHCP
    let platform = match detect_platform() {
        Some(p) => p,
        None => {
            // No platform adapter - just update state
            state.mode = NetworkMode::Dhcp;
            state.desired = None;
            state.active = None;
            save_network_state(state).await?;
            return Ok(());
        }
    };

    // Get interface from desired config
    let interface = state
        .desired
        .as_ref()
        .map(|d| d.interface.clone())
        .unwrap_or_else(|| "eth0".to_string());

    // Apply DHCP
    platform.apply_dhcp(&interface).await?;

    // Update state
    state.mode = NetworkMode::Dhcp;
    state.desired = None;
    state.active = None;

    // Persist state
    save_network_state(state).await?;

    tracing::info!(
        interface = %interface,
        platform = platform.name(),
        "Reverted to DHCP (no static IP requesters remain)"
    );

    Ok(())
}

/// Detect primary network interface
fn detect_primary_interface() -> Option<String> {
    // Try to get the interface with default route
    // For now, use a simple heuristic based on common interface names
    #[cfg(target_os = "linux")]
    {
        // Check common interface names in order of preference
        let candidates = ["eth0", "ens0", "enp0s3", "eno1", "wlan0"];
        for candidate in candidates {
            let path = format!("/sys/class/net/{}", candidate);
            if std::path::Path::new(&path).exists() {
                return Some(candidate.to_string());
            }
        }

        // Fallback: list interfaces and pick first non-loopback
        if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "lo" && !name.starts_with("veth") && !name.starts_with("docker") && !name.starts_with("br-") {
                    return Some(name);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_ip_apply() {
        let apply = StaticIpApply {
            interface: "eth0".to_string(),
            address: "192.168.1.100".parse().unwrap(),
            prefix_length: 24,
            gateway: "192.168.1.1".parse().unwrap(),
            dns: vec!["8.8.8.8".parse().unwrap()],
        };

        assert_eq!(apply.interface, "eth0");
        assert_eq!(apply.prefix_length, 24);
    }
}
