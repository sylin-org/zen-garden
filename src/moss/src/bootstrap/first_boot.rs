//! First-time stone initialization
//!
//! Handles one-time setup for new stone installations:
//! - Generates unique stone name with collision detection
//! - Configures system hostname
//! - Updates configuration files
//! - Creates MOTD (message of the day)
//!
//! This runs when a stone boots with the default "stone-01" name.

use garden_common::PlatformRuntime;

/// Run first-boot initialization sequence
///
/// Displays progress on console, generates unique name, configures hostname, and creates MOTD.
///
/// # Arguments
/// * `runtime` - Platform runtime for console output
/// * `old_name` - Current temporary stone name (e.g., "stone-01")
/// * `port` - HTTP server port for management URL
///
/// # Returns
/// The newly generated stone name
///
/// # Process
/// 1. Generate unique stone name with collision detection
/// 2. Configure system hostname (updates /etc/hostname, /etc/hosts)
/// 3. Restart Avahi mDNS service
/// 4. Test mDNS resolution
/// 5. Update Moss configuration file
/// 6. Create MOTD with management URL
pub async fn run_first_boot_initialization(
    runtime: &dyn PlatformRuntime,
    old_name: &str,
    port: u16,
) -> anyhow::Result<String> {
    use garden_common::console;

    runtime.display_header("Zen Garden - First Boot");
    runtime.write_line("");
    runtime.display_item("Temporary Name", old_name);
    runtime.display_wait("Starting first-time setup");
    runtime.write_line("");

    // Generate unique name with collision detection
    runtime.display_header("Name Generation");
    let new_name = console::generate_unique_name(runtime).await?;
    runtime.write_line("");

    // Configure system hostname
    runtime.display_header("System Configuration");
    console::set_hostname(runtime, &new_name).await?;
    console::update_hosts_file(runtime, old_name, &new_name).await?;
    console::restart_avahi(runtime).await?;
    console::test_mdns_resolution(runtime, &new_name).await?;
    runtime.write_line("");

    // Update Moss configuration
    runtime.display_header("Moss Configuration");
    console::update_moss_config(runtime, &new_name).await?;
    runtime.write_line("");

    // MOTD is written on every startup by the hardware detection task once
    // capabilities are known — no write here.

    // Final summary
    let url = format!("http://{}:{}", console::get_local_ip_sync(), port);
    runtime.display_header("Setup Complete");
    runtime.display_item("Stone Name", &new_name);
    runtime.display_item("Management URL", &url);
    runtime.display_item("Username", garden_common::constants::STONE_USER);
    runtime.display_item("Password", garden_common::constants::STONE_PASSWORD);
    runtime.write_line("");
    runtime.display_success("Stone is ready for use");
    runtime.write_line("");

    Ok(new_name)
}
