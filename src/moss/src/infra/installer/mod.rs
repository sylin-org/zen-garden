//! Self-deploying Moss (BUILD-0003)
//!
//! Single binary handles fresh install, update, repair, OS provisioning,
//! and pre-start staged deployment. Shell scripts eliminated from critical path.
//!
//! Subcommands (all synchronous, no Tokio runtime):
//! - `garden-moss install [-y|--yes] [--dry-run]`
//! - `garden-moss uninstall`
//! - `garden-moss pre-start [--dry-run]`

#[cfg(target_os = "linux")]
pub(crate) mod linux;
mod package;
#[cfg(target_os = "linux")]
pub(crate) mod pre_start;
mod provision;
#[cfg(test)]
mod tests;
pub mod version;
#[cfg(target_os = "windows")]
mod windows;

use std::path::{Path, PathBuf};

use garden_common::infra::platform::is_running_from_removable_media;

use version::{InstallMethod, InstallMode, InstalledVersion};

/// Service name constants
#[cfg(target_os = "linux")]
const SERVICE_NAME: &str = "garden-moss";
#[cfg(target_os = "windows")]
const WINDOWS_SERVICE_NAME: &str = "ZenGardenMoss";
#[cfg(target_os = "windows")]
const WINDOWS_DISPLAY_NAME: &str = "Zen Garden Moss";
#[cfg(target_os = "windows")]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "Zen Garden stone orchestration daemon \u{2014} manages container services, \
     storage, and companions";

/// Firewall rule-name prefix (shared between install-time and runtime rules).
#[cfg(target_os = "windows")]
const FIREWALL_RULE_PREFIX: &str = "Zen Garden";

/// Legacy rule names from pre-v0.2 installs (cleaned up during install/uninstall).
#[cfg(target_os = "windows")]
const LEGACY_FIREWALL_RULES: &[&str] =
    &["Zen Garden Moss HTTP (TCP)", "Zen Garden Moss mDNS (UDP)"];

/// Options for the install command
#[derive(Default)]
pub struct InstallOptions {
    /// Accept all prompts without asking (for scripts/automation/USB)
    pub yes: bool,
    /// Show what would happen without making changes
    pub dry_run: bool,
}

/// Install Zen Garden as a system service.
///
/// Auto-detects fresh install vs update vs repair. Probes environment
/// for missing components and offers to install them interactively
/// (or auto-accepts with --yes).
pub fn install(options: &InstallOptions) -> anyhow::Result<()> {
    if !options.dry_run {
        check_privileges("install")?;
    }

    // Detect install mode
    let mode = InstallMode::detect(crate::cli::VERSION);

    println!();
    match &mode {
        InstallMode::Fresh => {
            println!("  Zen Garden Moss Installer");
            println!("  {}", crate::cli::VERSION);
        }
        InstallMode::Update { from, to } => {
            println!("  Zen Garden Moss Update");
            println!("  {} -> {}", from, to);
        }
        InstallMode::Repair { version } => {
            println!("  Zen Garden Moss Repair");
            println!("  {}", version);
        }
    }
    if options.dry_run {
        println!("  [DRY RUN]");
    }
    println!();

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine executable directory"))?;

    // If running from removable media, copy to permanent location first
    let (work_dir, copied_from_removable) = if options.dry_run {
        let is_removable = is_running_from_removable_media(&exe_path).unwrap_or(false);
        if is_removable {
            println!("  Would copy binary and package from removable media.");
        }
        (exe_dir.to_path_buf(), false)
    } else {
        resolve_work_directory(&exe_path, exe_dir)?
    };

    // Phase 1: Resolve package (interactive download prompt)
    println!("Resolving package...");
    let package_path = package::resolve_package(&work_dir, options.yes)?;

    // ── Dry run: report and exit ────────────────────────────────────
    if options.dry_run {
        print_dry_run(&mode, &package_path)?;
        return Ok(());
    }

    // Phase 2: Extract and install
    println!();
    match &mode {
        InstallMode::Fresh => println!("Installing Zen Garden..."),
        InstallMode::Update { .. } => println!("Updating Zen Garden..."),
        InstallMode::Repair { .. } => println!("Repairing Zen Garden..."),
    }

    let install_dir = platform_install_dir();
    std::fs::create_dir_all(&install_dir)?;

    let staging_handle = staging_directory()?;
    let staging_dir = staging_handle.path();

    println!("  Extracting package...");
    package::extract_package(&package_path, staging_dir)?;

    // On update: stop service before deploying files
    if matches!(
        mode,
        InstallMode::Update { .. } | InstallMode::Repair { .. }
    ) {
        stop_service_if_running();
    }

    // Platform-specific installation
    #[cfg(target_os = "linux")]
    linux::install_platform(staging_dir)?;

    #[cfg(target_os = "windows")]
    windows::install_platform(staging_dir, &work_dir)?;

    // Create default config (fresh install only)
    #[cfg(target_os = "linux")]
    if matches!(mode, InstallMode::Fresh) {
        create_default_config()?;
    }

    // Write version breadcrumb
    let breadcrumb = InstalledVersion::new(crate::cli::VERSION, InstallMethod::Install);
    if let Err(e) = version::write_installed_version(&breadcrumb) {
        println!("  Warning: could not write version breadcrumb: {e}");
    }

    // Staging directory auto-cleans when `staging_handle` drops (end of function
    // or on error). Explicit drop here to clean up before the next phase.
    drop(staging_handle);

    // Cleanup removable media temp files
    if copied_from_removable {
        println!("  Cleaning up temporary files...");
        let _ = cleanup_removable_temp(&work_dir);
    }

    // Phase 3: Environment check and provisioning
    run_environment_check(options)?;

    // Phase 4: Start and verify
    println!();
    start_and_verify()?;

    Ok(())
}

/// Process pre-staged packages before daemon start.
///
/// Called as `ExecStartPre=/usr/local/bin/garden-moss pre-start`.
/// Replaces `moss-update-helper.sh`.
#[cfg(target_os = "linux")]
pub fn pre_start(dry_run: bool) -> anyhow::Result<()> {
    pre_start::run(dry_run)
}

/// Uninstall Zen Garden service and binaries. Data is preserved.
pub fn uninstall() -> anyhow::Result<()> {
    check_privileges("uninstall")?;

    println!();
    println!("Uninstalling Zen Garden...");
    println!();

    #[cfg(target_os = "linux")]
    linux::uninstall_platform()?;

    #[cfg(target_os = "windows")]
    windows::uninstall_platform()?;

    // Print data preservation notice
    let data = garden_common::constants::paths::data_dir();
    let config = garden_common::constants::paths::config_dir();

    println!();
    println!("Zen Garden has been removed.");
    println!();
    println!("  Data preserved at:   {}", data);
    println!("  Config preserved at: {}", config);
    println!();

    #[cfg(target_os = "linux")]
    println!("  To remove all data: sudo rm -rf {} {}", data, config);
    #[cfg(target_os = "windows")]
    println!("  To remove all data, delete these directories manually.");

    println!();

    Ok(())
}

// ── Environment check and provisioning ──────────────────────────────

fn run_environment_check(options: &InstallOptions) -> anyhow::Result<()> {
    println!();
    println!("Environment check:");

    let components = provision::probe();

    // Display status of each component
    for component in &components {
        let icon = match &component.status {
            provision::ComponentStatus::Ok => "ok",
            provision::ComponentStatus::Missing => "not installed",
            provision::ComponentStatus::Manual(hint) => hint.as_str(),
        };
        println!("  {:<20} {}", component.name, icon);
    }

    if provision::all_ok(&components) {
        // Nothing to do
        return Ok(());
    }

    // Check if there are any auto-fixable components
    if !provision::needs_provisioning(&components) {
        // Only manual items remain
        println!();
        println!("  Some components require manual setup (see above).");
        return Ok(());
    }

    // Prompt user (or auto-accept with --yes)
    let should_provision = if options.yes {
        println!();
        println!("  Setting up missing components (--yes)...");
        true
    } else {
        println!();
        print!("  Set up missing components? [Y/n] ");
        prompt_yes_no(true)
    };

    if should_provision {
        println!();
        provision::apply()?;
    } else {
        println!();
        println!("  Skipped. You can re-run with: garden-moss install");
    }

    Ok(())
}

// ── Dry run report ──────────────────────────────────────────────────

fn print_dry_run(mode: &InstallMode, package_path: &Path) -> anyhow::Result<()> {
    println!();
    println!("Dry run — the following actions would be performed:");
    println!();
    #[cfg(target_os = "linux")]
    println!("  NOTE: Actual install requires root (sudo garden-moss install)");
    #[cfg(target_os = "windows")]
    println!("  NOTE: Actual install requires Administrator privileges");
    println!();
    println!("  Mode: {}", mode);
    println!("  Package: {}", package_path.display());
    println!("  Install dir: {}", platform_install_dir().display());

    #[cfg(target_os = "linux")]
    {
        println!("  Service: systemd unit at /etc/systemd/system/garden-moss.service");
        println!("  ExecStartPre: /usr/local/bin/garden-moss pre-start");
        if matches!(mode, InstallMode::Fresh) {
            println!(
                "  Config: default garden-moss.toml at {}",
                garden_common::constants::paths::config_dir()
            );
        }
    }

    // Show environment check in dry-run too
    println!();
    println!("  Environment check:");
    let components = provision::probe();
    for component in &components {
        let icon = match &component.status {
            provision::ComponentStatus::Ok => "ok",
            provision::ComponentStatus::Missing => "WOULD INSTALL",
            provision::ComponentStatus::Manual(hint) => hint.as_str(),
        };
        println!("    {:<20} {}", component.name, icon);
    }

    println!();
    println!("  Would start service and verify health.");
    println!();
    Ok(())
}

// ── Interactive prompts ─────────────────────────────────────────────

/// Prompt for yes/no with a default. Returns true for yes.
pub(super) fn prompt_yes_no(default_yes: bool) -> bool {
    use std::io::{self, BufRead, Write};

    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().lock().read_line(&mut input).is_err() {
        return default_yes;
    }

    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return default_yes;
    }

    matches!(trimmed.as_str(), "y" | "yes")
}

// ── Default config ──────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn create_default_config() -> anyhow::Result<()> {
    let config_dir = garden_common::constants::paths::config_dir();
    let config_path = format!("{config_dir}/garden-moss.toml");

    if Path::new(&config_path).exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        &config_path,
        format!(
            "# garden-moss configuration\n\
             \n\
             port = {}\n\
             log_level = \"info\"\n",
            garden_common::constants::MOSS_HTTP
        ),
    )?;

    println!("  Default config: {config_path}");
    Ok(())
}

// ── Service control ─────────────────────────────────────────────────

/// Stop the OS service if it is currently running.
///
/// **Synchronous only** — uses `std::thread::sleep`. Must never be called from
/// inside a Tokio runtime (the installer subcommands run without a runtime).
fn stop_service_if_running() {
    #[cfg(target_os = "linux")]
    {
        let is_active = std::process::Command::new("systemctl")
            .args(["is-active", "--quiet", SERVICE_NAME])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if is_active {
            print!("  Stopping service...");
            let _ = std::process::Command::new("systemctl")
                .args(["stop", SERVICE_NAME])
                .output();
            println!(" done.");
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("sc")
            .args(["query", WINDOWS_SERVICE_NAME])
            .output();

        if let Ok(o) = output {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if stdout.contains("RUNNING") {
                print!("  Stopping service...");
                let _ = std::process::Command::new("sc")
                    .args(["stop", WINDOWS_SERVICE_NAME])
                    .output();
                std::thread::sleep(std::time::Duration::from_secs(2));
                println!(" done.");
            }
        }
    }
}

// ── Privilege checks ────────────────────────────────────────────────

fn check_privileges(verb: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("id").arg("-u").output();
        match output {
            Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "0" => {}
            _ => anyhow::bail!(
                "garden-moss {verb} requires root \u{2014} try: sudo garden-moss {verb}"
            ),
        }
    }

    #[cfg(target_os = "windows")]
    {
        let ok = std::process::Command::new("net")
            .arg("session")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ok {
            anyhow::bail!(
                "garden-moss {verb} requires Administrator privileges \u{2014} \
                 right-click your terminal and choose \"Run as administrator\""
            );
        }
    }

    Ok(())
}

// ── Removable media handling ────────────────────────────────────────

fn resolve_work_directory(exe_path: &Path, exe_dir: &Path) -> anyhow::Result<(PathBuf, bool)> {
    let is_removable = is_running_from_removable_media(exe_path)?;

    if !is_removable {
        return Ok((exe_dir.to_path_buf(), false));
    }

    println!("  Detected removable media, copying to permanent location...");

    let temp_dir = install_temp_dir()?;

    let target_exe = temp_dir.join(exe_path.file_name().unwrap_or_default());
    std::fs::copy(exe_path, &target_exe)?;

    if let Ok(entries) = std::fs::read_dir(exe_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("zen-garden-")
                && (name.ends_with(".tar.gz") || name.ends_with(".zip"))
            {
                let dest = temp_dir.join(&name);
                std::fs::copy(entry.path(), &dest)?;
                println!("  Copied: {}", name);
            }
        }
    }

    Ok((temp_dir, true))
}

fn cleanup_removable_temp(temp_dir: &Path) -> anyhow::Result<()> {
    // Only clean up directories we created (prefixed with zen-garden-install-)
    if let Some(name) = temp_dir.file_name().and_then(|n| n.to_str())
        && name.starts_with("zen-garden-install-") {
            std::fs::remove_dir_all(temp_dir)?;
        }
    Ok(())
}

// ── Platform paths ──────────────────────────────────────────────────

fn platform_install_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/usr/local/bin")
    }
    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        PathBuf::from(format!(r"{}\ZenGarden", program_data))
    }
}

fn install_temp_dir() -> anyhow::Result<PathBuf> {
    let dir = tempfile::Builder::new()
        .prefix("zen-garden-install-")
        .tempdir()?;
    // Persist the directory (caller manages cleanup) — don't auto-delete on drop
    let path = dir.keep();
    Ok(path)
}

/// Create a staging directory that auto-cleans on drop.
///
/// Returns a `TempDir` handle — the caller uses `.path()` for the directory
/// path, and the directory is automatically removed when the handle is dropped,
/// even on error paths. Explicit cleanup via `std::fs::remove_dir_all` is not
/// needed (but harmless if called).
fn staging_directory() -> anyhow::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("zen-garden-staging-")
        .tempdir()
        .map_err(Into::into)
}

// ── Start and verify ────────────────────────────────────────────────

/// Start the OS service and poll for health.
///
/// **Synchronous only** — uses `std::thread::sleep`. Must never be called from
/// inside a Tokio runtime (the installer subcommands run without a runtime).
fn start_and_verify() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        print!("Starting Moss...");
        let output = std::process::Command::new("systemctl")
            .args(["start", SERVICE_NAME])
            .output();
        match output {
            Ok(o) if o.status.success() => println!(" started."),
            Ok(o) => {
                println!();
                println!(
                    "  Warning: could not start service: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                );
            }
            Err(e) => {
                println!();
                println!("  Warning: could not start service: {e}");
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        print!("Starting Moss...");
        let output = std::process::Command::new("sc")
            .args(["start", WINDOWS_SERVICE_NAME])
            .output();
        match output {
            Ok(o) if o.status.success() => println!(" started."),
            Ok(o) => {
                println!();
                let stderr = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                println!(
                    "  Warning: could not start service: {} {}",
                    stdout.trim(),
                    stderr.trim()
                );
            }
            Err(e) => {
                println!();
                println!("  Warning: could not start service: {e}");
            }
        }
    }

    // Health check
    println!();
    print!("Checking health...");
    let port = garden_common::constants::MOSS_HTTP;
    let health_url = format!("http://localhost:{}/health", port);
    let mut healthy = false;

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(response) = ureq_get(&health_url)
            && response {
                healthy = true;
                break;
            }
    }

    if healthy {
        println!(" healthy.");
    } else {
        println!(" not yet responding (this is normal during first boot).");
    }

    println!();
    println!("Zen Garden is ready.");
    println!();
    println!("  API:     http://localhost:{}", port);
    println!("  Health:  {}", health_url);
    println!("  CLI:     garden-rake status");
    println!();

    #[cfg(target_os = "linux")]
    {
        println!("  Manage the service:");
        println!("    systemctl status garden-moss      View status");
        println!("    systemctl stop garden-moss        Stop");
        println!("    systemctl restart garden-moss     Restart");
        println!("    journalctl -u garden-moss -f      Follow logs");
    }

    #[cfg(target_os = "windows")]
    {
        println!("  Manage the service:");
        println!("    sc query ZenGardenMoss            View status");
        println!("    sc stop ZenGardenMoss             Stop");
        println!("    sc start ZenGardenMoss            Start");
    }

    println!();
    Ok(())
}

/// Simple blocking HTTP GET that returns true if status is 2xx.
fn ureq_get(url: &str) -> anyhow::Result<bool> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{}", path);

    let mut stream = TcpStream::connect(host_port)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;

    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_port
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    Ok(response.starts_with("HTTP/1.") && response.contains(" 200 "))
}
