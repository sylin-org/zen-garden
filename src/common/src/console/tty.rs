//! TTY and first-boot console infrastructure

use std::fs::OpenOptions;
use std::io::Write;
use anyhow::{Context, Result};

/// Ensure /etc is writable with retries for early-boot timing issues
/// Returns Ok(true) if writeable, Ok(false) if permanently read-only
pub async fn ensure_etc_writable() -> Result<bool> {
    const MAX_RETRIES: u32 = 10;
    const RETRY_DELAY_MS: u64 = 500;
    
    let test_path = "/etc/.moss-write-test";
    
    for attempt in 1..=MAX_RETRIES {
        match std::fs::write(test_path, "test") {
            Ok(_) => {
                // Writable - cleanup test file
                let _ = std::fs::remove_file(test_path);
                if attempt > 1 {
                    tracing::info!(attempt, "/etc became writable after retries");
                }
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied || 
                       e.raw_os_error() == Some(30) => { // EROFS = 30
                
                if attempt == 1 {
                    tracing::warn!("/etc is not yet writable, will retry (may be early boot timing)");
                }
                
                // On first attempt, try remounting
                if attempt == 1 {
                    let output = tokio::process::Command::new("mount")
                        .args(["-o", "remount,rw", "/"])
                        .output()
                        .await;
                    
                    if let Ok(result) = output {
                        if result.status.success() {
                            tracing::info!("Attempted remount of root filesystem as read-write");
                        }
                    }
                }
                
                // Wait before retry unless it's the last attempt
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                } else {
                    tracing::error!(
                        attempts = MAX_RETRIES,
                        "/etc remained read-only after all retries"
                    );
                    return Ok(false);
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Unexpected error testing /etc writability: {}", e));
            }
        }
    }
    
    Ok(false)
}

/// Write text directly to TTY1 console
/// Falls back to stdout if TTY not available
pub fn tty_write(text: &str) -> Result<()> {
    // Try to open /dev/tty1 for writing
    match OpenOptions::new()
        .write(true)
        .open("/dev/tty1")
    {
        Ok(mut tty) => {
            writeln!(tty, "{}", text)
                .context("Failed to write to /dev/tty1")?;
            tty.flush()
                .context("Failed to flush TTY")?;
        }
        Err(_) => {
            // Fallback to stdout (for testing or non-Linux systems)
            println!("{}", text);
        }
    }
    Ok(())
}

/// Display a header with box frame
/// Example:
/// ╔══════════════════════════════════════╗
/// ║       Zen Garden - First Boot        ║
/// ╚══════════════════════════════════════╝
pub fn display_header(title: &str) -> Result<()> {
    let width = 40;
    let padding = (width - title.len() - 2) / 2;
    let extra = if (width - title.len() - 2) % 2 == 1 { 1 } else { 0 };
    
    let top = format!("╔{}╗", "═".repeat(width - 2));
    let middle = format!("║{}{}{}║", 
        " ".repeat(padding),
        title,
        " ".repeat(padding + extra)
    );
    let bottom = format!("╚{}╝", "═".repeat(width - 2));
    
    tty_write("")?;
    tty_write(&top)?;
    tty_write(&middle)?;
    tty_write(&bottom)?;
    tty_write("")?;
    Ok(())
}

/// Display an item with simple indentation
/// Example: "  Stone Name: stone-meadow-42"
pub fn display_item(label: &str, value: &str) -> Result<()> {
    tty_write(&format!("  {}: {}", label, value))
}

/// Display a success message with [OK] indicator
/// Example: "  [OK] Docker daemon connected"
pub fn display_success(message: &str) -> Result<()> {
    tty_write(&format!("  [OK] {}", message))
}

/// Display an error message with [FAIL] indicator
/// Example: "  [FAIL] Failed to generate name"
pub fn display_error(message: &str) -> Result<()> {
    tty_write(&format!("  [FAIL] {}", message))
}

/// Display a waiting/progress message with [WAIT] indicator
/// Example: "  [WAIT] Checking name availability..."
pub fn display_wait(message: &str) -> Result<()> {
    tty_write(&format!("  [WAIT] {}", message))
}

/// Check if this is the first run by looking for the initialization flag file
pub fn is_first_run() -> bool {
    !std::path::Path::new(&crate::constants::paths::first_run_flag()).exists()
}

/// Mark first-run initialization as complete
pub async fn mark_first_run_complete() -> Result<()> {
    tokio::fs::write(crate::constants::paths::first_run_flag(), "")
        .await
        .context("Failed to create first-run completion flag")?;
    Ok(())
}

/// Generate a unique stone name with collision detection
/// 
/// Uses adjective-noun pattern with mDNS collision checking (10 attempts).
/// Falls back to hex suffix if all attempts collide.
pub async fn generate_unique_name() -> Result<String> {
    const ADJECTIVES: &[&str] = &[
        "azure", "bronze", "coral", "crimson", "emerald", "golden", "indigo",
        "jade", "lunar", "marble", "obsidian", "pearl", "quartz", "ruby",
        "silver", "topaz", "turquoise", "violet", "amber", "crystal"
    ];
    
    const NOUNS: &[&str] = &[
        "meadow", "summit", "river", "forest", "canyon", "valley", "harbor",
        "glacier", "prairie", "desert", "delta", "ridge", "plateau", "grove",
        "basin", "stream", "cliff", "shore", "peak", "dune"
    ];
    
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    // Use StdRng which is Send-safe for background tasks
    let mut rng = rand::rngs::StdRng::from_entropy();
    
    // Try 10 random combinations
    for attempt in 1..=10 {
        let adjective = ADJECTIVES.choose(&mut rng).unwrap();
        let noun = NOUNS.choose(&mut rng).unwrap();
        let candidate = format!("stone-{}-{}", adjective, noun);
        
        display_wait(&format!("Checking availability: {} (attempt {}/10)", candidate, attempt))?;
        
        // Check mDNS collision
        if !check_mdns_collision(&candidate).await {
            display_success(&format!("Name available: {}", candidate))?;
            return Ok(candidate);
        }
        
        display_wait(&format!("Name collision detected: {}", candidate))?;
    }
    
    // All attempts failed, use hex suffix
    let hex_suffix = format!("{:04x}", rand::random::<u16>());
    let fallback = format!("stone-{}", hex_suffix);
    display_wait(&format!("Using fallback name: {}", fallback))?;
    Ok(fallback)
}

/// Check if a stone name already exists on the network via mDNS
/// Returns true if collision detected, false if available
async fn check_mdns_collision(name: &str) -> bool {
    // Query mDNS for _moss._tcp.local with instance name matching stone name
    // Timeout after 2 seconds
    let mdns_name = format!("{}._moss._tcp.local", name);
    
    // Use avahi-browse to check for existing service
    match tokio::process::Command::new("avahi-browse")
        .args(["-t", "-r", "-p", "_moss._tcp"])
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Check if our stone name appears in the output
            stdout.contains(&mdns_name) || stdout.contains(name)
        }
        Err(_) => {
            // avahi-browse not available or failed, assume no collision
            false
        }
    }
}

/// Set system hostname by writing directly to /etc/hostname
pub async fn set_hostname(name: &str) -> Result<()> {
    display_wait(&format!("Setting hostname to {}", name))?;
    
    // Write directly to /etc/hostname (more reliable than hostnamectl with NoNewPrivileges)
    tokio::fs::write("/etc/hostname", format!("{}\n", name))
        .await
        .context("Failed to write /etc/hostname")?;
    
    // Also set the running hostname using sethostname syscall
    // This requires the CAP_SYS_ADMIN capability but works with NoNewPrivileges
    let output = tokio::process::Command::new("hostname")
        .arg(name)
        .output()
        .await
        .context("Failed to execute hostname command")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        display_error(&format!("Warning: hostname command failed: {}", stderr))?;
        // Don't fail completely - the file write succeeded
    }
    
    display_success(&format!("Hostname set to {}", name))?;
    Ok(())
}

/// Read the system hostname from /etc/hostname.
///
/// This is the source of truth for what will be announced over mDNS (`<hostname>.local`).
pub async fn get_hostname() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        // Windows: Use ComputerName from environment or hostname command
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            if !name.is_empty() {
                return Ok(name.to_lowercase());
            }
        }
        
        // Fallback: Use hostname command
        match tokio::process::Command::new("hostname").output().await {
            Ok(output) if output.status.success() => {
                let hostname = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !hostname.is_empty() {
                    return Ok(hostname.to_lowercase());
                }
            }
            _ => {}
        }
        
        anyhow::bail!("Failed to get Windows hostname");
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        let content = tokio::fs::read_to_string("/etc/hostname")
            .await
            .context("Failed to read /etc/hostname")?;
        let hostname = content.trim().to_string();
        if hostname.is_empty() {
            anyhow::bail!("/etc/hostname was empty");
        }
        Ok(hostname)
    }
}

/// Update /etc/hosts to reflect a hostname change.
///
/// Uses word-boundary matching to only replace complete hostnames, not substrings.
/// This prevents the bug where "stone" matches "stone-golden-summit" and causes
/// concatenation like "stone-new-name-golden-summit".
pub async fn update_hosts_file(old_name: &str, new_name: &str) -> Result<()> {
    display_wait("Updating /etc/hosts")?;

    // Read current hosts file
    let hosts_content = tokio::fs::read_to_string("/etc/hosts")
        .await
        .context("Failed to read /etc/hosts")?;

    // Replace only complete hostname entries, not substrings.
    // /etc/hosts format: <IP address>   <hostname> [aliases...]
    // Hostnames are whitespace-delimited, so we split and match exactly.
    let updated_content = hosts_content
        .lines()
        .map(|line| {
            // Skip comment lines
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                return line.to_string();
            }

            // Split line into parts (IP and hostnames)
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return line.to_string();
            }

            // Check if any hostname field exactly matches old_name or legacy pattern
            let needs_update = parts.iter().skip(1).any(|&hostname| {
                hostname == old_name || hostname.starts_with("stone-new-")
            });

            if !needs_update {
                return line.to_string();
            }

            // Rebuild line with exact replacements (preserving original whitespace is tricky,
            // so we use single-space separation which is valid for /etc/hosts)
            let updated_parts: Vec<String> = parts.iter().enumerate().map(|(i, &part)| {
                if i == 0 {
                    // IP address - keep as-is
                    part.to_string()
                } else if part == old_name {
                    // Exact match - replace
                    new_name.to_string()
                } else if part.starts_with("stone-new-") {
                    // Legacy stone-new-* pattern - replace entire hostname
                    new_name.to_string()
                } else {
                    // Other hostnames (aliases) - keep as-is
                    part.to_string()
                }
            }).collect();

            // Use tab separator (common in /etc/hosts)
            format!("{}\t{}", updated_parts[0], updated_parts[1..].join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Write back
    tokio::fs::write("/etc/hosts", updated_content)
        .await
        .context("Failed to write /etc/hosts")?;

    display_success("Updated /etc/hosts")?;
    Ok(())
}

/// Restart avahi-daemon to update mDNS announcements
pub async fn restart_avahi() -> Result<()> {
    display_wait("Restarting avahi-daemon")?;
    
    let output = tokio::process::Command::new("systemctl")
        .args(["restart", "avahi-daemon"])
        .output()
        .await
        .context("Failed to restart avahi-daemon")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Don't fail - avahi restart is optional
        display_error(&format!("Warning: avahi restart failed: {}", stderr))?;
        tty_write("  (mDNS will update on next system reboot)")?;
    } else {
        display_success("Avahi daemon restarted")?;
    }
    Ok(())
}

/// Test mDNS resolution by pinging the stone's hostname
pub async fn test_mdns_resolution(stone_name: &str) -> Result<()> {
    display_wait(&format!("Testing mDNS resolution for {}.local", stone_name))?;
    
    // Wait a moment for avahi to propagate the announcement
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Try to ping the .local hostname (single ping, 2 second timeout)
    let hostname = format!("{}.local", stone_name);
    let output = tokio::process::Command::new("ping")
        .args(["-c", "1", "-W", "2", &hostname])
        .output()
        .await
        .context("Failed to execute ping command")?;
    
    if output.status.success() {
        display_success(&format!("mDNS resolution confirmed: {}.local is reachable", stone_name))?;
    } else {
        display_error(&format!("Warning: {}.local not yet reachable via mDNS", stone_name))?;
        tty_write("  (May take a few moments for network propagation)")?;
    }
    
    Ok(())
}

/// Write MOTD (Message of the Day) file
pub fn write_motd(stone_name: &str, url: &str) -> Result<()> {
    display_wait("Creating message of the day")?;
    
    let motd_content = format!(
r#"
╔══════════════════════════════════════╗
║       Zen Garden Stone Ready         ║
╚══════════════════════════════════════╝

  Stone Name: {}
  Management URL: {}
  Username: stone
  Password: garden

  Run 'systemctl status garden-moss' to check service status
  Visit {} to manage services

"#,
        stone_name,
        url,
        url
    );
    
    std::fs::write("/etc/motd", motd_content)
        .context("Failed to write /etc/motd")?;
    
    display_success("Message of the day created")?;
    Ok(())
}

/// Update Moss configuration file with new stone name
pub async fn update_moss_config(new_name: &str) -> Result<()> {
    display_wait("Updating Moss configuration")?;
    
    let config_dir = crate::constants::paths::config_dir();
    let config_path = format!("{}/{}", config_dir, crate::constants::MOSS_CONFIG);
    
    // Read current config
    let config_content = tokio::fs::read_to_string(&config_path)
        .await
        .context(format!("Failed to read {}", crate::constants::MOSS_CONFIG))?;
    
    let mut found = false;
    let mut updated_lines: Vec<String> = Vec::new();
    for line in config_content.lines() {
        let trimmed = line.trim();

        // Preferred modern key
        if trimmed.starts_with("stone_name") {
            let indent = line.len() - line.trim_start().len();
            updated_lines.push(format!("{}stone_name = \"{}\"", " ".repeat(indent), new_name));
            found = true;
            continue;
        }

        // Legacy key used in older templates
        if trimmed.starts_with("name =") || trimmed.starts_with("name=") {
            let indent = line.len() - line.trim_start().len();
            updated_lines.push(format!("{}name = \"{}\"", " ".repeat(indent), new_name));
            found = true;
            continue;
        }

        updated_lines.push(line.to_string());
    }

    // If neither key existed, insert a modern stone_name near the top (after any header comments).
    if !found {
        let mut inserted = false;
        let mut with_insert: Vec<String> = Vec::new();
        for line in &updated_lines {
            if !inserted {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    with_insert.push(line.clone());
                    continue;
                }
                with_insert.push(format!("stone_name = \"{}\"", new_name));
                inserted = true;
            }
            with_insert.push(line.clone());
        }
        if !inserted {
            with_insert.push(format!("stone_name = \"{}\"", new_name));
        }
        updated_lines = with_insert;
    }

    let updated_content = updated_lines.join("\n");
    
    // Write back
    tokio::fs::write(&config_path, updated_content)
        .await
        .context(format!("Failed to write moss.toml"))?;
    
    display_success("Configuration updated")?;
    Ok(())
}

/// Get local IP address synchronously (for use in non-async contexts)
///
/// Delegates to infra::network::get_local_ip() for consistent behavior.
/// Prefers LAN addresses (192.168.x.x, 10.x.x.x) over Docker bridge (172.17.x.x).
pub fn get_local_ip_sync() -> String {
    crate::infra::network::get_local_ip()
}

// ================================================================================================
// BOOT/SHUTDOWN BANNERS
// ================================================================================================

/// Boot banner info for display after READY
pub struct BootBannerInfo {
    pub stone_name: String,
    pub version: String,
    pub ip: String,
    pub port: u16,
    pub manifests_count: usize,
}

/// Print boot banner to TTY1 after System READY
///
/// Shows stone identity with a waking cat and time-aware greeting.
/// Called once after bootstrap completes successfully.
pub fn print_boot_banner(info: &BootBannerInfo) -> Result<()> {
    
    
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    let greeting = time_greeting();
    let symbol = boot_symbol();

    // Waking cat with dynamic symbol and stone name
    let cat_line1 = format!("    _|\\_/|    {:9} Stone: {}", symbol, info.stone_name);
    let cat_line2 = format!("  c(>(^-^)              This stone awakens... {}!", greeting);

    tty_write("")?;
    tty_write(divider)?;
    tty_write(&cat_line1)?;
    tty_write(&cat_line2)?;
    tty_write(divider)?;
    tty_write("")?;

    Ok(())
}

/// Shutdown banner info
pub struct ShutdownBannerInfo {
    pub stone_name: String,
    pub start_time: std::time::Instant,
}

/// Print shutdown banner to TTY1 before stopping
///
/// Shows graceful shutdown status with uptime and a sleepy cat.
pub fn print_shutdown_banner(info: &ShutdownBannerInfo) -> Result<()> {
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    let uptime_secs = info.start_time.elapsed().as_secs();
    let uptime_str = crate::utils::format_uptime(uptime_secs);
    let greeting = time_greeting();

    // Cat ASCII art with aligned right-side text starting at column 20
    let cat_line1 = format!("    _|\\_/|    ZZZzzz    Uptime: {}", uptime_str);
    let cat_line2 = format!("  c(-(-.-)              This stone rests... {}", greeting);

    tty_write("")?;
    tty_write(divider)?;
    tty_write(&cat_line1)?;
    tty_write(&cat_line2)?;
    tty_write(divider)?;
    tty_write("")?;

    Ok(())
}

/// Try to print boot banner to TTY1 (Linux only, no-op elsewhere)
///
/// Logs errors at debug level rather than failing.
pub fn try_boot_banner(info: Option<&BootBannerInfo>) {
    #[cfg(target_os = "linux")]
    if let Some(info) = info {
        if let Err(e) = print_boot_banner(info) {
            tracing::debug!(error = ?e, "Failed to print boot banner to TTY1");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = info;
}

/// Try to print shutdown banner to TTY1 (Linux only, no-op elsewhere)
///
/// Logs errors at debug level rather than failing.
pub fn try_shutdown_banner(info: Option<&ShutdownBannerInfo>) {
    #[cfg(target_os = "linux")]
    if let Some(info) = info {
        if let Err(e) = print_shutdown_banner(info) {
            tracing::debug!(error = ?e, "Failed to print shutdown banner to TTY1");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = info;
}

// ================================================================================================
// HELPER FUNCTIONS
// ================================================================================================

/// Get time-aware greeting based on current hour
fn time_greeting() -> &'static str {
    use chrono::{Local, Timelike};
    let hour = Local::now().hour();
    match hour {
        5..=11 => "good morning",
        12..=17 => "good afternoon",
        18..=20 => "good evening",
        _ => "good night", // 21-4
    }
}

/// Check if it's daytime (6am-6pm)
fn is_daytime() -> bool {
    use chrono::{Local, Timelike};
    let hour = Local::now().hour();
    (6..18).contains(&hour)
}

/// Get a random boot symbol based on time of day
fn boot_symbol() -> &'static str {
    use chrono::{Local, Timelike};
    let symbols_day = ["    *    ", "  c[_]   ", "~stretch~"];
    let symbols_night = ["    c    ", " *dimly* ", "  ~yawn~ "];
    
    // Use current second as simple randomizer
    let idx = (Local::now().second() as usize) % 3;
    
    if is_daytime() {
        symbols_day[idx]
    } else {
        symbols_night[idx]
    }
}

// ============================================================================
// Storage Ribbon Functions
// ============================================================================

/// Print seed bank detection ribbon to TTY1
/// 
/// Displays a visual notification when a USB storage device is detected.
/// Matches the visual style of existing boot/shutdown ribbons.
pub fn print_storage_detected_ribbon(info: &crate::storage::StorageDetectedInfo) -> Result<()> {
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    
    let label = info.label.as_deref().unwrap_or("USB Storage");
    let capacity = crate::utils::format_bytes(info.capacity_bytes);
    
    // USB icon with storage info
    let line1 = format!("    ┌──┐      🌱          Device: {} ({})", label, capacity);
    let line2 = "    │▓▓│                  A new seed bank awaits...";
    let line3 = "    └┬─┘";
    let line4 = "     │        Prepare:    garden-rake prepare seed-bank";
    
    tty_write("")?;
    tty_write(divider)?;
    tty_write(&line1)?;
    tty_write(line2)?;
    tty_write(line3)?;
    tty_write(line4)?;
    tty_write(divider)?;
    tty_write("")?;
    
    Ok(())
}

/// Print seed bank detection ribbon for multiple devices
pub fn print_storage_multi_ribbon(devices: &[crate::storage::StorageDetectedInfo]) -> Result<()> {
    if devices.is_empty() {
        return Ok(());
    }
    
    if devices.len() == 1 {
        return print_storage_detected_ribbon(&devices[0]);
    }
    
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    
    // Header
    let line1 = format!("    ┌──┐      🌱          {} devices await preparation", devices.len());
    let line2 = "    │▓▓│                  ";
    let line3 = "    └┬─┘";
    
    tty_write("")?;
    tty_write(divider)?;
    tty_write(&line1)?;
    tty_write(line2)?;
    tty_write(line3)?;
    
    // List each device
    for (i, dev) in devices.iter().enumerate() {
        let label = dev.label.as_deref().unwrap_or("USB Storage");
        let capacity = crate::utils::format_bytes(dev.capacity_bytes);
        let mount = dev.mount_path.as_deref().unwrap_or(&dev.device);
        let prefix = if i == devices.len() - 1 { "     │" } else { "     │" };
        tty_write(&format!("{}                    {} ({}) at {}", prefix, label, capacity, mount))?;
    }
    
    // Footer with command hint using first device label
    let first_label = devices[0].label.as_deref().unwrap_or(&devices[0].device);
    tty_write("     │")?;
    tty_write(&format!("     │        Prepare:    garden-rake prepare seed-bank {}", first_label))?;
    tty_write(divider)?;
    tty_write("")?;
    
    Ok(())
}

/// Print seed bank prepared confirmation ribbon
pub fn print_storage_prepared_ribbon(name: &str, mount_path: &str) -> Result<()> {
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    
    let line1 = format!("    ┌──┐      ✓           Seed bank ready: {}", name);
    let line2 = format!("    │▓▓│                  Mounted at: {}", mount_path);
    let line3 = "    └┬─┘";
    let line4 = "     │        Release:    garden-rake release seed-bank";
    
    tty_write("")?;
    tty_write(divider)?;
    tty_write(&line1)?;
    tty_write(&line2)?;
    tty_write(line3)?;
    tty_write(line4)?;
    tty_write(divider)?;
    tty_write("")?;
    
    Ok(())
}

/// Print seed bank released confirmation
pub fn print_storage_released_ribbon(name: &str) -> Result<()> {
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

    let line1 = format!("    ┌──┐      ↓           Seed bank released: {}", name);
    let line2 = "    │  │                  Safe to remove device";
    let line3 = "    └──┘";

    tty_write("")?;
    tty_write(divider)?;
    tty_write(&line1)?;
    tty_write(line2)?;
    tty_write(line3)?;
    tty_write(divider)?;
    tty_write("")?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    /// Test helper: transform a single hosts file line (mirrors update_hosts_file logic)
    fn transform_hosts_line(line: &str, old_name: &str, new_name: &str) -> String {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return line.to_string();
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return line.to_string();
        }

        let needs_update = parts.iter().skip(1).any(|&hostname| {
            hostname == old_name || hostname.starts_with("stone-new-")
        });

        if !needs_update {
            return line.to_string();
        }

        let updated_parts: Vec<String> = parts.iter().enumerate().map(|(i, &part)| {
            if i == 0 {
                part.to_string()
            } else if part == old_name {
                new_name.to_string()
            } else if part.starts_with("stone-new-") {
                new_name.to_string()
            } else {
                part.to_string()
            }
        }).collect();

        format!("{}\t{}", updated_parts[0], updated_parts[1..].join(" "))
    }

    #[test]
    fn test_hosts_line_basic_replacement() {
        // Basic case: replace "stone" with "stone-golden-summit"
        let line = "127.0.1.1\tstone";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_no_substring_replacement() {
        // Bug fix test: "stone" should NOT match "stone-golden-summit"
        // This was the root cause of the hostname concatenation bug
        let line = "127.0.1.1\tstone-golden-summit";
        let result = transform_hosts_line(line, "stone", "stone-crimson-glacier");
        // Should remain unchanged because "stone" != "stone-golden-summit"
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_exact_match_only() {
        // Should only replace exact matches
        let line = "127.0.1.1\tstone stone-alias";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        // Only "stone" should be replaced, not "stone-alias"
        assert_eq!(result, "127.0.1.1\tstone-golden-summit stone-alias");
    }

    #[test]
    fn test_hosts_line_legacy_stone_new_pattern() {
        // Legacy pattern: stone-new-* should be replaced
        let line = "127.0.1.1\tstone-new-abc123";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_preserve_comments() {
        // Comments should be preserved
        let line = "# This is a comment with stone in it";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "# This is a comment with stone in it");
    }

    #[test]
    fn test_hosts_line_preserve_localhost() {
        // localhost line should be unchanged
        let line = "127.0.0.1\tlocalhost";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.0.1\tlocalhost");
    }

    #[test]
    fn test_hosts_line_no_double_concatenation() {
        // Ensure running replacement twice doesn't cause concatenation
        let line = "127.0.1.1\tstone";
        let after_first = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(after_first, "127.0.1.1\tstone-golden-summit");

        // Running again with "stone" as old_name should NOT modify the line
        let after_second = transform_hosts_line(&after_first, "stone", "stone-crimson-glacier");
        assert_eq!(after_second, "127.0.1.1\tstone-golden-summit"); // Unchanged!
    }
}
