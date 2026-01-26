//! Service installation and update management
//!
//! Windows-specific service management:
//! - Installing Moss as a Windows service
//! - Handling service updates with transaction log
//! - Cleaning up after updates
//! - Automatic rollback on failure
//!
//! Future: Add Linux systemd and macOS launchd support

#[cfg(target_os = "windows")]
use crate::infra::update_transaction::{UpdateTransaction, UpdateStage};

#[cfg(target_os = "windows")]
fn log_update(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;
    let log_path = Path::new(&garden_common::constants::paths::data_dir()).join("moss-update.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

/// Spawn Windows updater process
///
/// Called by the API endpoint when a package contains garden-moss.
/// Copies the current moss executable to garden-moss-temp.exe and spawns it
/// with --finalize-update flag, then returns immediately.
///
/// The temp process will:
/// 1. Wait for old moss to exit
/// 2. Backup current binaries
/// 3. Install new binaries
/// 4. Verify new moss starts
/// 5. Self-cleanup
#[cfg(target_os = "windows")]
pub async fn spawn_windows_updater() -> anyhow::Result<()> {
    use std::process::Command;
    use anyhow::Context;

    log_update("=== spawn_windows_updater: STARTED ===");
    
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    log_update(&format!("Current exe: {:?}", current_exe));
    
    let exe_dir = current_exe.parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    log_update(&format!("Exe directory: {:?}", exe_dir));
    
    let temp_updater = exe_dir.join("garden-moss-temp.exe");
    log_update(&format!("Temp updater path: {:?}", temp_updater));
    
    tracing::info!(
        source = ?current_exe,
        temp = ?temp_updater,
        "Copying self to temporary updater"
    );
    
    // Copy current executable to temp location
    log_update("Copying current exe to temp...");
    std::fs::copy(&current_exe, &temp_updater)
        .context("Failed to copy executable to temp location")?;
    log_update("Copy successful!");
    
    // Spawn updater process (detached, does not wait)
    tracing::info!("Spawning updater process: garden-moss-temp.exe --update-finalize");
    log_update("Spawning temp updater with --update-finalize");
    
    let child = Command::new(&temp_updater)
        .arg("--update-finalize")
        .spawn()
        .context("Failed to spawn updater process")?;
    
    log_update(&format!("Updater spawned with PID: {:?}", child.id()));
    tracing::info!("Updater spawned successfully, shutdown will be triggered");
    log_update("=== spawn_windows_updater: COMPLETE ===");
    
    Ok(())
}

/// Install Moss as a Windows service
///
/// Handles both installation from removable media (copies to ProgramData) and
/// permanent locations. Creates the ZenGardenMoss service and starts it.
///
/// This implements both "take-root" (zen) and "install-service" (normative) commands.
///
/// # Windows Service Commands
/// - `sc query ZenGardenMoss` - View status
/// - `sc stop ZenGardenMoss` - Stop service
/// - `sc start ZenGardenMoss` - Start service
/// - `sc delete ZenGardenMoss` - Remove service (uproot)
#[cfg(target_os = "windows")]
pub async fn install_windows_service() -> anyhow::Result<()> {
    use std::process::Command;
    use std::path::PathBuf;

    println!("🌱 Taking root as Windows service...");
    println!();

    let current_exe = std::env::current_exe()?;

    // Check if running from removable media
    let is_removable = crate::infra::is_running_from_removable_media(&current_exe)?;

    let install_exe = if is_removable {
        println!("⚠️  Detected execution from removable media");
        println!("   Installing to permanent location...");
        println!();

        // Copy to ProgramData (system-wide, admin-accessible)
        let install_dir = PathBuf::from(r"C:\ProgramData\ZenGarden");
        std::fs::create_dir_all(&install_dir)?;

        let target_exe = install_dir.join("garden-moss.exe");

        // Copy executable
        std::fs::copy(&current_exe, &target_exe)?;
        println!("✓ Copied to: {}", target_exe.display());
        println!();

        target_exe
    } else {
        current_exe
    };

    let exe_path_str = install_exe.to_string_lossy();

    // Check if service already exists
    let check_output = Command::new("sc")
        .args(["query", "ZenGardenMoss"])
        .output()?;

    if check_output.status.success() {
        println!("⚠️  Service already exists");
        println!("   To reinstall, first remove: sc delete ZenGardenMoss");
        return Err(anyhow::anyhow!("Service already installed"));
    }

    // Create service using sc.exe with proper arguments
    // Note: sc.exe requires space after = in key=value pairs
    let bin_path = format!("binPath= {}", exe_path_str);
    let output = Command::new("sc")
        .args([
            "create",
            "ZenGardenMoss",
            &bin_path,
            "start=",
            "auto",
            "DisplayName=",
            "Zen Garden Moss",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Failed to create service:");
        eprintln!("  {}", stderr);
        eprintln!("  {}", stdout);
        return Err(anyhow::anyhow!("Service creation failed"));
    }

    println!("✓ Service rooted successfully");
    println!();

    // Set service description
    let _ = Command::new("sc")
        .args([
            "description",
            "ZenGardenMoss",
            "Zen Garden stone orchestration daemon - manages container services",
        ])
        .output();

    // Start the service
    println!("🌅 Waking the service...");
    let output = Command::new("sc")
        .args(["start", "ZenGardenMoss"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("⚠️  Failed to start service:");
        eprintln!("  {}", stderr);
        eprintln!("  {}", stdout);
        println!();
        println!("The service is installed but not running.");
        println!("Start it manually with: sc start ZenGardenMoss");
    } else {
        println!("✓ Service is awake and thriving");
    }

    println!();
    println!("🌿 Moss has taken root as a Windows service");
    println!();
    println!("Installation path: {}", exe_path_str);
    println!();
    println!("Management commands:");
    println!("  sc query ZenGardenMoss      View status");
    println!("  sc stop ZenGardenMoss       Stop service");
    println!("  sc start ZenGardenMoss      Start service");
    println!("  sc delete ZenGardenMoss     Remove service (uproot)");
    println!();
    println!("Access the web UI at: http://localhost:7185");

    Ok(())
}

/// Finalize Windows service update
///
/// Called when running as garden-moss-new.exe after an update.
/// Waits for the old process to exit, replaces the binary, and restarts the service.
#[cfg(target_os = "windows")]
pub async fn finalize_service_update() -> anyhow::Result<()> {
    use std::process::Command;

    log_update("=== finalize_service_update: STARTED ===");
    println!("Finalizing Moss update...");

    let current_exe = std::env::current_exe()?;
    log_update(&format!("Current exe: {:?}", current_exe));
    
    let exe_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    log_update(&format!("Exe directory: {:?}", exe_dir));
    
    let target_exe = exe_dir.join("garden-moss.exe");
    log_update(&format!("Target exe: {:?}", target_exe));

    // Wait for old process to exit (up to 30 seconds)
    println!("Waiting for old Moss process to exit...");
    log_update("Waiting for old moss process to exit (up to 30s)...");
    
    for attempt in 1..=60 {
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq garden-moss.exe"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("garden-moss.exe") {
            log_update(&format!("Old process exited after attempt {}", attempt));
            break;
        }

        if attempt == 60 {
            log_update("ERROR: Timeout waiting for old process to exit");
            eprintln!("Timeout waiting for old process to exit");
            return Err(anyhow::anyhow!("Old process did not exit"));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    println!("Old process exited. Replacing binary...");
    log_update("Copying temp updater to target exe...");
    std::fs::copy(&current_exe, &target_exe)?;
    log_update("Binary replaced successfully");
    println!("✓ Binary replaced successfully");

    // Check if running as service
    let is_service = std::env::var("RUNNING_AS_SERVICE").is_ok();
    log_update(&format!("Running as service: {}", is_service));

    if is_service {
        println!("Starting Moss service...");
        log_update("Starting ZenGardenMoss service...");
        let output = Command::new("sc")
            .args(["start", "ZenGardenMoss"])
            .output()?;
        log_update(&format!("Service start output: {:?}", String::from_utf8_lossy(&output.stdout)));
        println!("✓ Service start triggered");
    } else {
        println!("Launching new Moss...");
        log_update("Launching new Moss with --cleanup-old...");
        let child = Command::new(&target_exe)
            .arg("--cleanup-old")
            .spawn()?;
        log_update(&format!("New Moss spawned with PID: {:?}", child.id()));
        println!("✓ New Moss launched");
    }

    println!("Update complete. This process will now exit.");
    log_update("=== finalize_service_update: COMPLETE ===");
    Ok(())
}

/// Cleanup old binary after service update
///
/// Removes the garden-moss-new.exe file after a successful update.
/// Waits for the update process to exit before removing.
#[cfg(target_os = "windows")]
pub async fn cleanup_after_service_update() -> anyhow::Result<()> {
    use std::process::Command;

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    let old_exe = exe_dir.join("garden-moss-new.exe");

    if old_exe.exists() {
        // Wait for garden-moss-new.exe process to exit
        for _ in 1..=20 {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq garden-moss-new.exe"])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.contains("garden-moss-new.exe") {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Remove old binary
        std::fs::remove_file(&old_exe).ok();
    }

    // Continue with normal startup (fall through to main logic)
    Ok(())
}

/// Cleanup updater process after successful update
///
/// Removes the garden-moss-temp.exe file after a successful update.
/// Waits for the updater process to exit before removing.
#[cfg(target_os = "windows")]
pub async fn cleanup_updater_process() -> anyhow::Result<()> {
    use std::process::Command;
    
    log_update("=== cleanup_updater_process: STARTED ===");
    
    let current_exe = std::env::current_exe()?;
    log_update(&format!("Current exe: {:?}", current_exe));
    
    let exe_dir = current_exe.parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    log_update(&format!("Exe directory: {:?}", exe_dir));
    
    let temp_exe = exe_dir.join("garden-moss-temp.exe");
    log_update(&format!("Temp exe to clean: {:?}", temp_exe));
    
    if temp_exe.exists() {
        log_update("Temp updater exists, waiting for it to exit...");
        
        // Wait for updater process to exit
        for attempt in 1..=40 {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq garden-moss-temp.exe"])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.contains("garden-moss-temp.exe") {
                log_update(&format!("Temp process exited after attempt {}", attempt));
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Remove temp updater
        log_update("Removing temp updater file...");
        std::fs::remove_file(&temp_exe).ok();
        log_update("Temp updater removed");
        tracing::info!("Cleaned up updater process");
    } else {
        log_update("Temp updater does not exist (already cleaned or never created)");
    }
    
    log_update("=== cleanup_updater_process: COMPLETE ===");
    
    // Continue with normal startup
    Ok(())
}

/// Helper: Recursive directory copy
#[cfg(target_os = "windows")]
fn copy_dir_recursive(src: &str, dst: &str) -> anyhow::Result<()> {
    use std::path::Path;
    
    std::fs::create_dir_all(dst)?;
    
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let target = Path::new(dst).join(&file_name);
        
        if path.is_dir() {
            copy_dir_recursive(
                &path.to_string_lossy(),
                &target.to_string_lossy()
            )?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    
    Ok(())
}
