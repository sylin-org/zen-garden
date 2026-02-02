//! IP conflict probing
//!
//! Detects IP conflicts before applying static IP configuration.
//! Uses multiple methods for reliability:
//! 1. ARP probe (RFC 5227) - primary method
//! 2. ICMP ping - fallback for environments where ARP fails
//! 3. Local binding check - ensure IP isn't already used locally
//!
//! ## RFC 5227 ARP Probing
//!
//! An ARP probe is an ARP request with:
//! - Sender IP = 0.0.0.0 (to avoid polluting ARP caches)
//! - Target IP = the IP we want to check
//!
//! If we receive an ARP reply, the IP is in use.

use crate::domain::ProbeResult;
use std::net::Ipv4Addr;
use std::time::Duration;

/// Probe configuration
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    /// Timeout for ARP probe (RFC 5227 recommends 1s, we use 2s for reliability)
    pub arp_timeout: Duration,

    /// Timeout for ICMP ping
    pub ping_timeout: Duration,

    /// Number of ARP probes to send (RFC 5227 recommends 3)
    pub arp_probe_count: u32,

    /// Delay between ARP probes
    pub arp_probe_interval: Duration,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            arp_timeout: Duration::from_secs(2),
            ping_timeout: Duration::from_secs(1),
            arp_probe_count: 3,
            arp_probe_interval: Duration::from_millis(500),
        }
    }
}

/// Probe an IP for conflicts
///
/// Tries multiple methods to detect if an IP is in use:
/// 1. ARP probe (if available on platform)
/// 2. ICMP ping (fallback)
/// 3. Local binding check
///
/// Returns ProbeResult indicating whether the IP is available.
pub async fn probe_ip_conflict(
    ip: Ipv4Addr,
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    interface: &str,
    config: &ProbeConfig,
) -> ProbeResult {
    // 1. Check if IP is bound locally first (quick check)
    if is_ip_bound_locally(ip) {
        return ProbeResult::LocalConflict;
    }

    // 2. Try ARP probe (Linux only for now)
    #[cfg(target_os = "linux")]
    {
        match arp_probe(ip, interface, config).await {
            Ok(Some(mac)) => {
                return ProbeResult::Conflict {
                    method: "arp",
                    responder_mac: Some(mac),
                };
            }
            Ok(None) => {
                // No ARP reply - IP appears available
                // Continue to ICMP as additional verification
            }
            Err(e) => {
                tracing::debug!(ip = %ip, error = %e, "ARP probe failed, falling back to ICMP");
            }
        }
    }

    // 3. ICMP ping fallback
    if ping_probe(ip, config.ping_timeout).await {
        return ProbeResult::Conflict {
            method: "icmp",
            responder_mac: None,
        };
    }

    ProbeResult::Available
}

/// Check if IP is bound to a local interface
fn is_ip_bound_locally(ip: Ipv4Addr) -> bool {
    // Check if it's a loopback address
    if ip.is_loopback() {
        return true;
    }

    // Check local interfaces
    #[cfg(target_os = "linux")]
    {
        // Read from /proc/net/fib_trie or parse ip addr output
        if let Ok(output) = std::process::Command::new("ip")
            .args(["addr", "show"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let ip_str = ip.to_string();
            // Look for the IP in the output (inet X.X.X.X/prefix)
            if stdout.contains(&format!("inet {}/", ip_str)) || stdout.contains(&format!("inet {} ", ip_str)) {
                return true;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("ipconfig")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains(&ip.to_string()) {
                return true;
            }
        }
    }

    false
}

/// ARP probe implementation (Linux)
#[cfg(target_os = "linux")]
async fn arp_probe(
    ip: Ipv4Addr,
    interface: &str,
    config: &ProbeConfig,
) -> Result<Option<String>, String> {
    // Use arping command for ARP probing
    // arping -D -I <interface> -c <count> -w <timeout> <ip>
    // -D = duplicate address detection mode (sender IP = 0.0.0.0)

    let timeout_secs = config.arp_timeout.as_secs().max(1);

    let output = tokio::process::Command::new("arping")
        .args([
            "-D",                           // Duplicate detection mode
            "-I", interface,                // Interface
            "-c", &config.arp_probe_count.to_string(),  // Count
            "-w", &timeout_secs.to_string(), // Timeout
            &ip.to_string(),                // Target IP
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run arping: {}", e))?;

    // arping returns:
    // - exit code 0 if no reply (IP available)
    // - exit code 1 if got a reply (IP in use)

    if !output.status.success() {
        // Got a reply - IP is in use
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Try to extract MAC from output
        // Format: "Unicast reply from X.X.X.X [AA:BB:CC:DD:EE:FF]"
        let mac = stdout
            .lines()
            .find(|line| line.contains("reply from"))
            .and_then(|line| {
                let start = line.find('[')?;
                let end = line.find(']')?;
                Some(line[start + 1..end].to_string())
            });

        return Ok(mac);
    }

    // No reply - IP appears available
    Ok(None)
}

/// ICMP ping probe
async fn ping_probe(ip: Ipv4Addr, timeout: Duration) -> bool {
    let timeout_secs = timeout.as_secs().max(1);

    #[cfg(target_os = "linux")]
    let result = tokio::process::Command::new("ping")
        .args([
            "-c", "1",                      // Count
            "-W", &timeout_secs.to_string(), // Timeout
            &ip.to_string(),
        ])
        .output()
        .await;

    #[cfg(target_os = "windows")]
    let result = tokio::process::Command::new("ping")
        .args([
            "-n", "1",                      // Count
            "-w", &(timeout_secs * 1000).to_string(), // Timeout in ms
            &ip.to_string(),
        ])
        .output()
        .await;

    #[cfg(target_os = "macos")]
    let result = tokio::process::Command::new("ping")
        .args([
            "-c", "1",                      // Count
            "-t", &timeout_secs.to_string(), // Timeout
            &ip.to_string(),
        ])
        .output()
        .await;

    match result {
        Ok(output) => output.status.success(),
        Err(e) => {
            tracing::debug!(ip = %ip, error = %e, "Ping probe failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_config_default() {
        let config = ProbeConfig::default();
        assert_eq!(config.arp_timeout, Duration::from_secs(2));
        assert_eq!(config.ping_timeout, Duration::from_secs(1));
        assert_eq!(config.arp_probe_count, 3);
    }

    #[test]
    fn test_is_ip_bound_locally_loopback() {
        assert!(is_ip_bound_locally("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_ip_bound_locally_external() {
        // This should return false for an external IP not bound locally
        // (unless the test machine happens to have this IP)
        let ip: Ipv4Addr = "203.0.113.1".parse().unwrap(); // TEST-NET-3
        // Don't assert on this - it depends on local machine config
        let _ = is_ip_bound_locally(ip);
    }
}
