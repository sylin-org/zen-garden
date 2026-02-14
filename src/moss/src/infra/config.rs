//! Moss configuration management
//!
//! Provides centralized configuration loading, validation, and persistence.
//! Configuration is stored in TOML format at platform-specific locations.

use std::net::Ipv4Addr;

/// Moss daemon configuration
///
/// Configuration file format (TOML):
/// ```toml
/// stone_name = "stone-01"
/// port = 7185
/// log_level = "info"  # Options: trace, debug, info, warn, error
/// ```
///
/// File locations (first found wins):
/// - Linux: /etc/zen-garden/moss.toml
/// - Windows: ./moss.toml (current directory)
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MossConfig {
    /// Stone name identifier - default: "stone-01"
    pub stone_name: Option<String>,

    /// HTTP server port - default: 7185
    pub port: Option<u16>,

    /// Log level (trace/debug/info/warn/error) - default: "info"
    pub log_level: Option<String>,

    /// Fast sync timeout in seconds for rapid offering deployments - default: None (disabled)
    pub fast_sync_timeout: Option<u64>,

    /// Console output mode (silent/minimal/informative/verbose) - default: platform-specific
    pub console_mode: Option<String>,

    /// Event deduplication TTL in seconds - default: 10
    #[serde(default)]
    pub event_dedup_ttl_secs: Option<u64>,

    /// Docker connection retry delay in seconds - default: 3
    #[serde(default)]
    pub docker_retry_delay_secs: Option<u64>,

    /// Health check interval in seconds - default: 30
    #[serde(default)]
    pub health_check_interval_secs: Option<u64>,

    /// Docker reconnect interval in seconds - default: 5
    #[serde(default)]
    pub docker_reconnect_interval_secs: Option<u64>,

    /// HTTP capabilities fetch timeout in seconds - default: 5
    #[serde(default)]
    pub http_capabilities_timeout_secs: Option<u64>,

    /// HTTP health check timeout in seconds - default: 2
    #[serde(default)]
    pub http_health_timeout_secs: Option<u64>,

    /// HTTP quick health check timeout in milliseconds - default: 200
    #[serde(default)]
    pub http_quick_health_timeout_millis: Option<u64>,

    /// HTTP long operation timeout in seconds - default: 300 (5 minutes)
    #[serde(default)]
    pub http_long_operation_timeout_secs: Option<u64>,

    /// Adoption settings for adopted offerings
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub adoption: Option<AdoptionConfig>,

    /// Network configuration (static IP pool, etc.)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub network: Option<NetworkConfig>,
}

/// Scan schedule phase: (interval_secs, duration_secs)
/// - interval_secs: how often to scan during this phase
/// - duration_secs: how long this phase lasts (-1 = forever)
pub type ScanSchedulePhase = (u64, i64);

/// Adoption configuration for auto-detection and management
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct AdoptionConfig {
    /// Enable auto-adoption at bootstrap (default: true for regular, false for USB/container)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub enabled: Option<bool>,

    /// Default control level for adopted offerings (default: "monitor")
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_control_level: Option<String>,

    /// Exclude patterns for offerings to never adopt (regex patterns)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub exclude: Vec<String>,

    /// Detection cache TTL in seconds (default: 300)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detection_cache_ttl_secs: Option<u64>,

    /// Stability threshold - consecutive successes before adoption (default: 2)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stability_threshold: Option<u8>,

    /// Scan schedule: list of (interval_secs, duration_secs) phases
    /// Default: [[10, 600], [30, -1]] = 10s for 10min, then 30s forever
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_schedule: Option<Vec<ScanSchedulePhase>>,
}

impl AdoptionConfig {
    /// Check if adoption is enabled (with deployment profile detection)
    pub fn is_enabled(&self) -> bool {
        if let Some(enabled) = self.enabled {
            enabled
        } else {
            // Auto-detect deployment profile
            Self::default_enabled_for_deployment()
        }
    }

    /// Determine default adoption enabled state based on deployment profile
    fn default_enabled_for_deployment() -> bool {
        // Check for container deployment (ZEN_GARDEN_CONTAINER env var)
        if std::env::var("ZEN_GARDEN_CONTAINER").is_ok() {
            return false; // Container deployment: isolated, no host adoption
        }

        // Check for USB/removable media deployment
        if let Ok(exe_path) = std::env::current_exe() {
            if let Ok(is_removable) = crate::infra::is_running_from_removable_media(&exe_path) {
                if is_removable {
                    return false; // USB Moss: self-contained, no auto-adoption
                }
            }
        }

        // Regular deployment: enable auto-adoption by default
        true
    }

    /// Get default control level
    pub fn default_control_level(&self) -> &str {
        self.default_control_level.as_deref().unwrap_or("monitor")
    }

    /// Get detection cache TTL in seconds
    pub fn detection_cache_ttl_secs(&self) -> u64 {
        self.detection_cache_ttl_secs.unwrap_or(300)
    }

    /// Get stability threshold
    pub fn stability_threshold(&self) -> u8 {
        self.stability_threshold.unwrap_or(2)
    }

    /// Check if offering should be excluded from adoption
    pub fn is_excluded(&self, offering: &str) -> bool {
        use regex::Regex;

        for pattern in &self.exclude {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(offering) {
                    return true;
                }
            }
        }
        false
    }

    /// Get the scan schedule (default: 10s for 10min, then 30s forever)
    pub fn scan_schedule(&self) -> Vec<ScanSchedulePhase> {
        self.scan_schedule.clone().unwrap_or_else(|| {
            vec![
                (10, 600), // 10 second intervals for first 10 minutes
                (30, -1),  // 30 second intervals forever after
            ]
        })
    }

    /// Get the current scan interval based on elapsed time since start
    pub fn current_scan_interval(&self, elapsed_secs: u64) -> u64 {
        let schedule = self.scan_schedule();
        let mut accumulated_duration: u64 = 0;

        for (interval, duration) in schedule {
            if duration < 0 {
                // -1 means forever, this is the final phase
                return interval;
            }
            let phase_duration = duration as u64;
            if elapsed_secs < accumulated_duration + phase_duration {
                return interval;
            }
            accumulated_duration += phase_duration;
        }

        // Fallback to 30 seconds if schedule is empty
        30
    }
}

// ============================================================================
// Network Configuration
// ============================================================================

/// Network configuration section
///
/// Example TOML:
/// ```toml
/// [network.static_ip]
/// enabled = true
/// pool_start = "192.168.1.240"
/// pool_end = "192.168.1.250"
/// gateway = "192.168.1.1"
/// dns = ["8.8.8.8", "1.1.1.1"]
/// ```
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct NetworkConfig {
    /// Static IP pool configuration
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub static_ip: Option<StaticIpPoolConfig>,
}

/// Static IP pool configuration for automatic IP assignment
///
/// When an offering requests a static IP (e.g., Pi-hole for DNS stability),
/// Moss will select an available IP from this pool, probe for conflicts,
/// and apply it to the network interface.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct StaticIpPoolConfig {
    /// Enable static IP assignment from pool
    #[serde(default)]
    pub enabled: bool,

    /// First IP address in the pool (inclusive)
    pub pool_start: Ipv4Addr,

    /// Last IP address in the pool (inclusive)
    pub pool_end: Ipv4Addr,

    /// Default gateway for static IP configuration
    pub gateway: Ipv4Addr,

    /// DNS servers for static IP configuration
    #[serde(default)]
    pub dns: Vec<Ipv4Addr>,

    /// Network interface to configure (auto-detected if omitted)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub interface: Option<String>,

    /// Subnet prefix length (default: 24 for /24 network)
    #[serde(default = "default_prefix_length")]
    pub prefix_length: u8,
}

fn default_prefix_length() -> u8 {
    24
}

impl StaticIpPoolConfig {
    /// Check if the pool is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Auto-detect network configuration and create default pool
    ///
    /// Detects current network settings and creates a pool at the high end
    /// of the subnet (e.g., .240-.250 for a /24 network).
    ///
    /// Returns None if auto-detection fails.
    pub fn detect_defaults() -> Option<Self> {
        // Try to detect current network configuration
        let (current_ip, prefix_len, gateway, interface) = detect_current_network()?;

        // Calculate default pool range at high end of subnet
        // For /24: use .240-.250 (11 addresses)
        // For other prefixes: use last 11 addresses before broadcast
        let (pool_start, pool_end) = calculate_default_pool(current_ip, prefix_len);

        tracing::info!(
            current_ip = %current_ip,
            prefix_len = prefix_len,
            gateway = %gateway,
            interface = %interface,
            pool_start = %pool_start,
            pool_end = %pool_end,
            "Auto-detected network defaults for static IP pool"
        );

        Some(Self {
            enabled: true, // Auto-detected defaults are enabled by default
            pool_start,
            pool_end,
            gateway,
            dns: vec!["8.8.8.8".parse().unwrap(), "1.1.1.1".parse().unwrap()],
            interface: Some(interface),
            prefix_length: prefix_len,
        })
    }

    /// Get the pool size (number of addresses)
    pub fn pool_size(&self) -> u32 {
        let start: u32 = self.pool_start.into();
        let end: u32 = self.pool_end.into();
        if end >= start {
            end - start + 1
        } else {
            0
        }
    }

    /// Check if an IP is within the pool range
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        let ip_int: u32 = ip.into();
        let start: u32 = self.pool_start.into();
        let end: u32 = self.pool_end.into();
        ip_int >= start && ip_int <= end
    }

    /// Iterate over all IPs in the pool
    pub fn iter(&self) -> impl Iterator<Item = Ipv4Addr> {
        let start: u32 = self.pool_start.into();
        let end: u32 = self.pool_end.into();
        (start..=end).map(Ipv4Addr::from)
    }
}

impl MossConfig {
    /// Load configuration from platform-specific path
    ///
    /// Searches for garden-moss.toml at:
    /// - Linux: /etc/zen-garden/garden-moss.toml
    /// - Windows: ./.zen-garden/garden-moss.toml (current directory)
    ///
    /// Returns None if file not found or contains errors (falls back to defaults)
    pub fn load() -> Option<Self> {
        let config_path = std::path::PathBuf::from(garden_common::constants::CONFIG_DIR)
            .join(garden_common::constants::MOSS_CONFIG);

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<MossConfig>(&content) {
                Ok(config) => {
                    tracing::info!(
                        path = ?config_path,
                        stone_name = ?config.stone_name,
                        port = ?config.port,
                        log_level = ?config.log_level,
                        fast_sync_timeout = ?config.fast_sync_timeout,
                        "Loaded configuration from file"
                    );
                    // Console event emitted later in main() after console printer is available
                    Some(config)
                }
                Err(e) => {
                    tracing::warn!(path = ?config_path, error = ?e, "Failed to parse config file");
                    // Console event: Config | PARSE_ERROR emitted in main() as NotFound
                    None
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(path = ?config_path, "Config file not found, using defaults");
                // Console event: Config | NOT_FOUND emitted in main()
                None
            }
            Err(e) => {
                tracing::warn!(path = ?config_path, error = ?e, "Failed to read config file");
                // Console event: Config | READ_ERROR emitted in main() as NotFound
                None
            }
        }
    }

    /// Get event deduplication TTL in seconds (default: 10)
    pub fn event_dedup_ttl_secs(&self) -> u64 {
        self.event_dedup_ttl_secs.unwrap_or(10)
    }

    /// Get Docker retry delay in seconds (default: 3)
    pub fn docker_retry_delay_secs(&self) -> u64 {
        self.docker_retry_delay_secs.unwrap_or(3)
    }

    /// Get health check interval in seconds (default: 30)
    pub fn health_check_interval_secs(&self) -> u64 {
        self.health_check_interval_secs.unwrap_or(30)
    }

    /// Get Docker reconnect interval in seconds (default: 5)
    pub fn docker_reconnect_interval_secs(&self) -> u64 {
        self.docker_reconnect_interval_secs.unwrap_or(5)
    }

    /// Get HTTP capabilities timeout in seconds (default: 5)
    pub fn http_capabilities_timeout_secs(&self) -> u64 {
        self.http_capabilities_timeout_secs.unwrap_or(5)
    }

    /// Get HTTP health timeout in seconds (default: 2)
    pub fn http_health_timeout_secs(&self) -> u64 {
        self.http_health_timeout_secs.unwrap_or(2)
    }

    /// Get HTTP quick health timeout in milliseconds (default: 200)
    pub fn http_quick_health_timeout_millis(&self) -> u64 {
        self.http_quick_health_timeout_millis.unwrap_or(200)
    }

    /// Get HTTP long operation timeout in seconds (default: 300)
    pub fn http_long_operation_timeout_secs(&self) -> u64 {
        self.http_long_operation_timeout_secs.unwrap_or(300)
    }

    /// Get adoption configuration (with defaults)
    pub fn adoption(&self) -> AdoptionConfig {
        self.adoption.clone().unwrap_or_default()
    }

    /// Get network configuration
    pub fn network(&self) -> Option<&NetworkConfig> {
        self.network.as_ref()
    }

    /// Get static IP pool configuration if enabled
    pub fn static_ip_pool(&self) -> Option<&StaticIpPoolConfig> {
        self.network
            .as_ref()
            .and_then(|n| n.static_ip.as_ref())
            .filter(|p| p.enabled)
    }

    /// Save configuration to platform-specific path
    ///
    /// Saves garden-moss.toml to:
    /// - Linux: /etc/zen-garden/garden-moss.toml
    /// - Windows: ./garden-moss.toml (current directory)
    ///
    /// Returns Ok(()) on success, Err on write failure
    pub fn save(&self) -> Result<(), std::io::Error> {
        let config_dir = std::path::PathBuf::from(garden_common::constants::CONFIG_DIR);
        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join(garden_common::constants::MOSS_CONFIG);

        let toml_content =
            toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;

        std::fs::write(&config_path, toml_content)?;

        tracing::info!(path = ?config_path, "Saved configuration to file");
        Ok(())
    }

    /// Get static IP pool configuration, using auto-detected defaults if not configured
    ///
    /// Priority:
    /// 1. Explicit config with enabled=true → use configured values
    /// 2. Explicit config with enabled=false → None (disabled)
    /// 3. No config → auto-detect defaults
    pub fn static_ip_pool_with_defaults(&self) -> Option<StaticIpPoolConfig> {
        match self.network.as_ref().and_then(|n| n.static_ip.as_ref()) {
            Some(pool) => {
                if pool.enabled {
                    Some(pool.clone())
                } else {
                    // Explicitly disabled
                    None
                }
            }
            None => {
                // No config - try auto-detection
                StaticIpPoolConfig::detect_defaults()
            }
        }
    }

    /// Get static IP pool from config or auto-detect (static version for when config is None)
    ///
    /// This handles the case where no config file exists at all.
    /// Falls back to auto-detection.
    pub fn get_static_ip_pool(config: Option<&Self>) -> Option<StaticIpPoolConfig> {
        match config {
            Some(cfg) => cfg.static_ip_pool_with_defaults(),
            None => StaticIpPoolConfig::detect_defaults(),
        }
    }
}

// ============================================================================
// Network Auto-Detection Helpers
// ============================================================================

/// Detect current network configuration
///
/// Returns (current_ip, prefix_length, gateway, interface_name) or None if detection fails.
fn detect_current_network() -> Option<(Ipv4Addr, u8, Ipv4Addr, String)> {
    #[cfg(target_os = "linux")]
    {
        detect_current_network_linux()
    }

    #[cfg(target_os = "windows")]
    {
        detect_current_network_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_current_network_linux() -> Option<(Ipv4Addr, u8, Ipv4Addr, String)> {
    use std::process::Command;

    // Get default route interface and gateway using `ip route`
    let route_output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;

    let route_str = String::from_utf8_lossy(&route_output.stdout);
    // Format: "default via 192.168.1.1 dev eth0 ..."
    let parts: Vec<&str> = route_str.split_whitespace().collect();

    let gateway_idx = parts.iter().position(|&s| s == "via")?;
    let gateway: Ipv4Addr = parts.get(gateway_idx + 1)?.parse().ok()?;

    let dev_idx = parts.iter().position(|&s| s == "dev")?;
    let interface = parts.get(dev_idx + 1)?.to_string();

    // Get IP address for the interface using `ip addr show <interface>`
    let addr_output = Command::new("ip")
        .args(["addr", "show", &interface])
        .output()
        .ok()?;

    let addr_str = String::from_utf8_lossy(&addr_output.stdout);
    // Find "inet X.X.X.X/Y" line
    for line in addr_str.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("inet ") && !trimmed.contains("inet6") {
            // Format: "inet 192.168.1.100/24 ..."
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if let Some(cidr) = parts.get(1) {
                let cidr_parts: Vec<&str> = cidr.split('/').collect();
                if cidr_parts.len() == 2 {
                    let ip: Ipv4Addr = cidr_parts[0].parse().ok()?;
                    let prefix: u8 = cidr_parts[1].parse().ok()?;
                    return Some((ip, prefix, gateway, interface));
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn detect_current_network_windows() -> Option<(Ipv4Addr, u8, Ipv4Addr, String)> {
    use std::process::Command;

    // Use PowerShell to get network configuration
    // Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway } | Select-Object -First 1
    let ps_script = r#"
        $config = Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway } | Select-Object -First 1
        if ($config) {
            $addr = $config.IPv4Address
            $gw = $config.IPv4DefaultGateway.NextHop
            $iface = $config.InterfaceAlias
            Write-Output "$($addr.IPAddress)|$($addr.PrefixLength)|$gw|$iface"
        }
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_script])
        .output()
        .ok()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = output_str.trim().split('|').collect();

    if parts.len() == 4 {
        let ip: Ipv4Addr = parts[0].parse().ok()?;
        let prefix: u8 = parts[1].parse().ok()?;
        let gateway: Ipv4Addr = parts[2].parse().ok()?;
        let interface = parts[3].to_string();
        return Some((ip, prefix, gateway, interface));
    }

    None
}

/// Calculate default pool range at high end of subnet
///
/// For a /24 network, returns (.240, .250) giving 11 addresses.
/// For other prefix lengths, calculates proportionally.
fn calculate_default_pool(current_ip: Ipv4Addr, prefix_len: u8) -> (Ipv4Addr, Ipv4Addr) {
    let ip_int: u32 = current_ip.into();

    // Calculate network address and broadcast address
    let host_bits = 32 - prefix_len;
    let network_mask: u32 = !((1u32 << host_bits) - 1);
    let network_addr = ip_int & network_mask;
    let broadcast_addr = network_addr | !network_mask;

    // Pool size: 11 addresses (or less for small subnets)
    let max_pool_size = 11u32;
    let available_hosts = broadcast_addr - network_addr - 1; // Exclude network and broadcast

    let pool_size = max_pool_size.min(available_hosts / 4); // Use at most 25% of subnet for pool

    // Place pool at high end of subnet, leaving 5 addresses before broadcast
    // This avoids common DHCP ranges which typically start low
    let pool_end_int = broadcast_addr - 5; // 5 addresses reserved at top
    let pool_start_int = pool_end_int.saturating_sub(pool_size - 1);

    // Ensure pool_start is at least network_addr + 1
    let pool_start_int = pool_start_int.max(network_addr + 1);

    (Ipv4Addr::from(pool_start_int), Ipv4Addr::from(pool_end_int))
}
