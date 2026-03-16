//! TTY and first-boot console infrastructure

use crate::PlatformRuntime;
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
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    || e.raw_os_error() == Some(30) =>
            {
                // EROFS = 30

                if attempt == 1 {
                    tracing::warn!(
                        "/etc is not yet writable, will retry (may be early boot timing)"
                    );
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
                return Err(anyhow::anyhow!(
                    "Unexpected error testing /etc writability: {}",
                    e
                ));
            }
        }
    }

    Ok(false)
}

// ================================================================================================
// RIBBON INFRASTRUCTURE
// ================================================================================================

/// Standard divider for ribbons (52 chars)
pub const RIBBON_DIVIDER: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

/// ASCII art line prefixes for cat (waking/sleeping share line 1)
pub mod ribbon_art {
    pub const CAT_HEAD: &str = "    _|\\_/|  ";
    pub const CAT_WAKING: &str = "  c(>(^-^)  ";
    pub const CAT_SLEEPING: &str = "  c(-(-.-)  ";
    pub const CAT_UPDATING: &str = "  c(>(o.O)  ";

    pub const USB_TOP_ACTIVE: &str = "   ╭───╮ // ";
    pub const USB_BODY_ACTIVE: &str = "   ╰───╯ \\\\ ";
    pub const USB_TOP_EMPTY: &str = "   ╭───╮ \\\\ ";
    pub const USB_BODY_EMPTY: &str = "   ╰───╯ // ";
}

// ================================================================================================
// BOOT/SHUTDOWN BANNERS — data only; rendering lives in PlatformRuntime (ARCH-0002)
// ================================================================================================

/// Boot banner info for display after READY
pub struct BootBannerInfo {
    pub stone_name: String,
    pub version: String,
    pub ip: String,
    pub port: u16,
    pub manifests_count: usize,
}

/// Shutdown banner info
pub struct ShutdownBannerInfo {
    pub stone_name: String,
    pub start_time: std::time::Instant,
}

/// Update banner info for software updates
pub struct UpdateBannerInfo {
    pub stone_name: String,
    pub new_version: Option<String>,
}

// ================================================================================================
// FIRST-BOOT SYSTEM HELPERS
// ================================================================================================

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
///
/// Platform-aware: Linux uses nature theme, Windows uses stained glass/clarity theme.
pub async fn generate_unique_name(runtime: &dyn PlatformRuntime) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        generate_unique_name_windows(runtime).await
    }

    #[cfg(target_os = "linux")]
    {
        generate_unique_name_linux(runtime).await
    }
}

/// Generate a unique stone name for Linux machines
///
/// Uses a "zen garden / nature" theme with landscapes and natural formations.
/// 64 adjectives × 64 nouns = 4,096 combinations
pub async fn generate_unique_name_linux(runtime: &dyn PlatformRuntime) -> Result<String> {
    const ADJECTIVES: &[&str] = &[
        // Precious materials (16)
        "amber",
        "azure",
        "bronze",
        "coral",
        "crimson",
        "crystal",
        "emerald",
        "golden",
        "indigo",
        "jade",
        "marble",
        "obsidian",
        "pearl",
        "quartz",
        "ruby",
        "silver",
        // Gemstone & mineral (16)
        "topaz",
        "turquoise",
        "violet",
        "onyx",
        "opal",
        "garnet",
        "sapphire",
        "copper",
        "ivory",
        "ebony",
        "platinum",
        "cobalt",
        "ochre",
        "slate",
        "granite",
        "basalt",
        // Natural qualities (16)
        "lunar",
        "solar",
        "stellar",
        "misty",
        "mossy",
        "frosty",
        "dusky",
        "verdant",
        "tranquil",
        "serene",
        "gentle",
        "silent",
        "ancient",
        "hidden",
        "sacred",
        "eternal",
        // Atmospheric (16)
        "wispy",
        "shimmering",
        "glowing",
        "sunlit",
        "moonlit",
        "shadowed",
        "dappled",
        "veiled",
        "halcyon",
        "placid",
        "limpid",
        "pristine",
        "radiant",
        "luminous",
        "muted",
        "hushed",
    ];

    const NOUNS: &[&str] = &[
        // Landforms (16)
        "meadow", "summit", "canyon", "valley", "ridge", "plateau", "basin", "cliff", "peak",
        "dune", "bluff", "mesa", "butte", "hollow", "knoll", "crag",
        // Water features (16)
        "river", "harbor", "glacier", "delta", "stream", "shore", "brook", "lagoon", "spring",
        "cascade", "rapids", "estuary", "inlet", "cove", "fjord", "atoll",
        // Vegetation zones (16)
        "forest", "prairie", "desert", "grove", "thicket", "copse", "glade", "heath", "fen", "moor",
        "marsh", "swamp", "taiga", "tundra", "steppe", "savanna",
        // Natural spaces (16)
        "clearing", "alcove", "grotto", "cavern", "ravine", "gorge", "chasm", "vale", "dell",
        "glen", "pass", "garden", "terrace", "oasis", "refuge", "haven",
    ];

    generate_unique_name_from_dictionary(runtime, ADJECTIVES, NOUNS).await
}

/// Generate a unique stone name for Windows machines
///
/// Uses a "stained glass / clarity / architectural" theme that plays on the
/// Windows name while staying zen-calm. Evokes cathedral windows, light,
/// transparency, and sacred spaces.
/// 64 adjectives × 64 nouns = 4,096 combinations
pub async fn generate_unique_name_windows(runtime: &dyn PlatformRuntime) -> Result<String> {
    const ADJECTIVES: &[&str] = &[
        // Clarity & Transparency (16)
        "clear",
        "lucid",
        "pellucid",
        "crystalline",
        "vitreous",
        "translucent",
        "limpid",
        "pristine",
        "pure",
        "unclouded",
        "polished",
        "refined",
        "flawless",
        "seamless",
        "diaphanous",
        "gossamer",
        // Stillness & Calm (16)
        "smooth",
        "still",
        "calm",
        "placid",
        "serene",
        "tranquil",
        "hushed",
        "muted",
        "gentle",
        "soft",
        "quiet",
        "silent",
        "peaceful",
        "restful",
        "composed",
        "poised",
        // Stained Glass Colors (16)
        "azure",
        "vermillion",
        "cobalt",
        "amber",
        "violet",
        "emerald",
        "crimson",
        "sapphire",
        "ochre",
        "sienna",
        "cerulean",
        "scarlet",
        "indigo",
        "teal",
        "magenta",
        "gilded",
        // Architectural & Ornate (16)
        "frosted",
        "stained",
        "arched",
        "latticed",
        "beveled",
        "etched",
        "leaded",
        "mullioned",
        "prismatic",
        "opalescent",
        "iridescent",
        "lustrous",
        "radiant",
        "burnished",
        "vaulted",
        "tracery",
    ];

    const NOUNS: &[&str] = &[
        // Clarity & Stillness (16)
        "clarity",
        "purity",
        "stillness",
        "essence",
        "reflection",
        "surface",
        "depth",
        "mirror",
        "pool",
        "spring",
        "fountain",
        "stream",
        "ripple",
        "whisper",
        "breath",
        "pause",
        // Sacred Spaces (16)
        "chapel",
        "sanctuary",
        "cloister",
        "nave",
        "alcove",
        "niche",
        "grotto",
        "shrine",
        "vestry",
        "chancel",
        "transept",
        "apse",
        "atrium",
        "portico",
        "ambulatory",
        "clerestory",
        // Glass & Light Elements (16)
        "shard",
        "fragment",
        "tessera",
        "facet",
        "jewel",
        "gem",
        "pane",
        "sill",
        "aperture",
        "vista",
        "portal",
        "threshold",
        "prism",
        "lens",
        "spectrum",
        "refraction",
        // Light Phenomena (16)
        "gleam",
        "glow",
        "aurora",
        "dawn",
        "radiance",
        "lucidity",
        "brilliance",
        "luminance",
        "glimmer",
        "shimmer",
        "sparkle",
        "luster",
        "halo",
        "corona",
        "nimbus",
        "iridescence",
    ];

    generate_unique_name_from_dictionary(runtime, ADJECTIVES, NOUNS).await
}

/// Generate unique name from provided dictionaries
///
/// Shared implementation for platform-specific name generators.
/// Tries 10 random combinations with mDNS collision checking.
async fn generate_unique_name_from_dictionary(
    runtime: &dyn PlatformRuntime,
    adjectives: &[&str],
    nouns: &[&str],
) -> Result<String> {
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    // Use StdRng which is Send-safe for background tasks
    let mut rng = rand::rngs::StdRng::from_entropy();

    // Try 10 random combinations
    for attempt in 1..=10 {
        let adjective = adjectives.choose(&mut rng).unwrap();
        let noun = nouns.choose(&mut rng).unwrap();
        let candidate = format!("stone-{}-{}", adjective, noun);

        runtime.display_wait(&format!(
            "Checking availability: {} (attempt {}/10)",
            candidate, attempt
        ));

        // Check mDNS collision
        if !check_mdns_collision(&candidate).await {
            runtime.display_success(&format!("Name available: {}", candidate));
            return Ok(candidate);
        }

        runtime.display_wait(&format!("Name collision detected: {}", candidate));
    }

    // All attempts failed, use hex suffix
    let hex_suffix = format!("{:04x}", rand::random::<u16>());
    let fallback = format!("stone-{}", hex_suffix);
    runtime.display_wait(&format!("Using fallback name: {}", fallback));
    Ok(fallback)
}

/// Check if a stone name already exists on the network via mDNS
/// Returns true if collision detected, false if available
async fn check_mdns_collision(name: &str) -> bool {
    let mdns_name = format!(
        "{}.{}",
        name,
        crate::constants::MDNS_SERVICE_TYPE_LOCAL.trim_end_matches('.')
    );

    match tokio::process::Command::new("avahi-browse")
        .args(["-t", "-r", "-p", crate::constants::MDNS_SERVICE_TYPE])
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains(&mdns_name) || stdout.contains(name)
        }
        Err(_) => false,
    }
}

/// Set system hostname by writing directly to /etc/hostname
pub async fn set_hostname(runtime: &dyn PlatformRuntime, name: &str) -> Result<()> {
    runtime.display_wait(&format!("Setting hostname to {}", name));

    tokio::fs::write("/etc/hostname", format!("{}\n", name))
        .await
        .context("Failed to write /etc/hostname")?;

    let output = tokio::process::Command::new("hostname")
        .arg(name)
        .output()
        .await
        .context("Failed to execute hostname command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        runtime.display_error(&format!("Warning: hostname command failed: {}", stderr));
    }

    runtime.display_success(&format!("Hostname set to {}", name));
    Ok(())
}

/// Read the system hostname from /etc/hostname.
///
/// This is the source of truth for what will be announced over mDNS (`<hostname>.local`).
pub async fn get_hostname() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            if !name.is_empty() {
                return Ok(name.to_lowercase());
            }
        }

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

    #[cfg(target_os = "linux")]
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
pub async fn update_hosts_file(
    runtime: &dyn PlatformRuntime,
    old_name: &str,
    new_name: &str,
) -> Result<()> {
    runtime.display_wait("Updating /etc/hosts");

    let hosts_content = tokio::fs::read_to_string("/etc/hosts")
        .await
        .context("Failed to read /etc/hosts")?;

    let updated_content = hosts_content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                return line.to_string();
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return line.to_string();
            }

            let needs_update = parts
                .iter()
                .skip(1)
                .any(|&hostname| hostname == old_name || hostname.starts_with("stone-new-"));

            if !needs_update {
                return line.to_string();
            }

            let updated_parts: Vec<String> = parts
                .iter()
                .enumerate()
                .map(|(i, &part)| {
                    if i == 0 {
                        part.to_string()
                    } else if part == old_name || part.starts_with("stone-new-") {
                        new_name.to_string()
                    } else {
                        part.to_string()
                    }
                })
                .collect();

            format!("{}\t{}", updated_parts[0], updated_parts[1..].join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n");

    tokio::fs::write("/etc/hosts", updated_content)
        .await
        .context("Failed to write /etc/hosts")?;

    runtime.display_success("Updated /etc/hosts");
    Ok(())
}

/// Restart avahi-daemon to update mDNS announcements
pub async fn restart_avahi(runtime: &dyn PlatformRuntime) -> Result<()> {
    runtime.display_wait("Restarting avahi-daemon");

    let output = tokio::process::Command::new("systemctl")
        .args(["restart", "avahi-daemon"])
        .output()
        .await
        .context("Failed to restart avahi-daemon")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        runtime.display_error(&format!("Warning: avahi restart failed: {}", stderr));
        runtime.write_line("  (mDNS will update on next system reboot)");
    } else {
        runtime.display_success("Avahi daemon restarted");
    }
    Ok(())
}

/// Test mDNS resolution by pinging the stone's hostname
pub async fn test_mdns_resolution(runtime: &dyn PlatformRuntime, stone_name: &str) -> Result<()> {
    runtime.display_wait(&format!("Testing mDNS resolution for {}.local", stone_name));

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let hostname = format!("{}.local", stone_name);
    let output = tokio::process::Command::new("ping")
        .args(["-c", "1", "-W", "2", &hostname])
        .output()
        .await
        .context("Failed to execute ping command")?;

    if output.status.success() {
        runtime.display_success(&format!(
            "mDNS resolution confirmed: {}.local is reachable",
            stone_name
        ));
    } else {
        runtime.display_error(&format!(
            "Warning: {}.local not yet reachable via mDNS",
            stone_name
        ));
        runtime.write_line("  (May take a few moments for network propagation)");
    }

    Ok(())
}

/// Write MOTD (Message of the Day) file
pub fn write_motd(runtime: &dyn PlatformRuntime, stone_name: &str, url: &str) -> Result<()> {
    runtime.display_wait("Creating message of the day");

    let motd_content = format!(
        r#"
╔══════════════════════════════════════╗
║       Zen Garden Stone Ready         ║
╚══════════════════════════════════════╝

  Stone Name: {}
  Management URL: {}
  Username: stone
  Password: stone

  Run 'systemctl status garden-moss' to check service status
  Visit {} to manage services

"#,
        stone_name, url, url
    );

    std::fs::write("/etc/motd", motd_content).context("Failed to write /etc/motd")?;

    runtime.display_success("Message of the day created");
    Ok(())
}

/// Update Moss configuration file with new stone name
pub async fn update_moss_config(runtime: &dyn PlatformRuntime, new_name: &str) -> Result<()> {
    runtime.display_wait("Updating Moss configuration");

    let config_dir = crate::constants::paths::config_dir();
    let config_path = format!("{}/{}", config_dir, crate::constants::MOSS_CONFIG);

    let config_content = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(path = %config_path, "Config file not found, creating default");
            let default = format!(
                "# garden-moss configuration\n\nport = {}\nlog_level = \"info\"\n",
                crate::constants::MOSS_HTTP
            );
            let config_dir_path = std::path::Path::new(&config_dir);
            tokio::fs::create_dir_all(config_dir_path)
                .await
                .context("Failed to create config directory")?;
            tokio::fs::write(&config_path, &default)
                .await
                .context(format!(
                    "Failed to create default {}",
                    crate::constants::MOSS_CONFIG
                ))?;
            default
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to read {}: {}",
                crate::constants::MOSS_CONFIG,
                e
            ))
        }
    };

    let mut found = false;
    let mut updated_lines: Vec<String> = Vec::new();
    for line in config_content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("stone_name") {
            let indent = line.len() - line.trim_start().len();
            updated_lines.push(format!(
                "{}stone_name = \"{}\"",
                " ".repeat(indent),
                new_name
            ));
            found = true;
            continue;
        }

        if trimmed.starts_with("name =") || trimmed.starts_with("name=") {
            let indent = line.len() - line.trim_start().len();
            updated_lines.push(format!("{}name = \"{}\"", " ".repeat(indent), new_name));
            found = true;
            continue;
        }

        updated_lines.push(line.to_string());
    }

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

    tokio::fs::write(&config_path, updated_content)
        .await
        .context("Failed to write moss.toml".to_string())?;

    runtime.display_success("Configuration updated");
    Ok(())
}

/// Get local IP address synchronously (for use in non-async contexts)
///
/// Delegates to infra::network::get_local_ip() for consistent behavior.
/// Prefers LAN addresses (192.168.x.x, 10.x.x.x) over Docker bridge (172.17.x.x).
pub fn get_local_ip_sync() -> String {
    crate::infra::network::get_local_ip()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    fn transform_hosts_line(line: &str, old_name: &str, new_name: &str) -> String {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return line.to_string();
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return line.to_string();
        }

        let needs_update = parts
            .iter()
            .skip(1)
            .any(|&hostname| hostname == old_name || hostname.starts_with("stone-new-"));

        if !needs_update {
            return line.to_string();
        }

        let updated_parts: Vec<String> = parts
            .iter()
            .enumerate()
            .map(|(i, &part)| {
                if i == 0 {
                    part.to_string()
                } else if part == old_name {
                    new_name.to_string()
                } else if part.starts_with("stone-new-") {
                    new_name.to_string()
                } else {
                    part.to_string()
                }
            })
            .collect();

        format!("{}\t{}", updated_parts[0], updated_parts[1..].join(" "))
    }

    #[test]
    fn test_hosts_line_basic_replacement() {
        let line = "127.0.1.1\tstone";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_no_substring_replacement() {
        let line = "127.0.1.1\tstone-golden-summit";
        let result = transform_hosts_line(line, "stone", "stone-crimson-glacier");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_exact_match_only() {
        let line = "127.0.1.1\tstone stone-alias";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit stone-alias");
    }

    #[test]
    fn test_hosts_line_legacy_stone_new_pattern() {
        let line = "127.0.1.1\tstone-new-abc123";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.1.1\tstone-golden-summit");
    }

    #[test]
    fn test_hosts_line_preserve_comments() {
        let line = "# This is a comment with stone in it";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "# This is a comment with stone in it");
    }

    #[test]
    fn test_hosts_line_preserve_localhost() {
        let line = "127.0.0.1\tlocalhost";
        let result = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(result, "127.0.0.1\tlocalhost");
    }

    #[test]
    fn test_hosts_line_no_double_concatenation() {
        let line = "127.0.1.1\tstone";
        let after_first = transform_hosts_line(line, "stone", "stone-golden-summit");
        assert_eq!(after_first, "127.0.1.1\tstone-golden-summit");

        let after_second = transform_hosts_line(&after_first, "stone", "stone-crimson-glacier");
        assert_eq!(after_second, "127.0.1.1\tstone-golden-summit");
    }
}
