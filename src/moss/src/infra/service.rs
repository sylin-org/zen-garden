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
    use anyhow::Context;
    use std::process::Command;

    log_update("=== spawn_windows_updater: STARTED ===");

    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
    log_update(&format!("Current exe: {:?}", current_exe));

    let exe_dir = current_exe
        .parent()
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
    use std::path::PathBuf;
    use std::process::Command;

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
    println!(
        "Access the web UI at: http://localhost:{}",
        garden_common::constants::MOSS_HTTP
    );

    Ok(())
}

/// Finalize Windows service update
///
/// Called when running as garden-moss-temp.exe after an update.
/// Waits for the old process to exit, installs staged binaries, and restarts.
#[cfg(target_os = "windows")]
pub async fn finalize_service_update() -> anyhow::Result<()> {
    use std::path::Path;
    use std::process::Command;

    log_update("=== finalize_service_update: STARTED ===");
    println!("Finalizing Moss update...");

    let current_exe = std::env::current_exe()?;
    log_update(&format!("Current exe: {:?}", current_exe));

    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    log_update(&format!("Exe directory (install dir): {:?}", exe_dir));

    // Staged binaries are in data_dir/staging/validated/bin/
    let staging_bin_dir = Path::new(&garden_common::constants::paths::data_dir())
        .join("staging")
        .join("validated")
        .join("bin");
    log_update(&format!("Staging bin dir: {:?}", staging_bin_dir));

    if !staging_bin_dir.exists() {
        log_update("ERROR: Staging bin directory does not exist");
        return Err(anyhow::anyhow!(
            "Staging bin directory not found: {:?}",
            staging_bin_dir
        ));
    }

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

    // Pre-copy: stop external tool services so their binaries can be overwritten.
    // Tools run as Windows services — the file is locked while the service is active.
    // We run uninstall from the *installed* binary (the one the service is actually running).
    let existing_tools_dir = Path::new(&garden_common::constants::paths::data_dir()).join("tools");
    if existing_tools_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&existing_tools_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let tool_path = entry.path();
                if tool_path.is_file() && tool_path.extension().map(|e| e == "exe").unwrap_or(false)
                {
                    let tool_name = tool_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    log_update(&format!("Stopping external tool service: {}", tool_name));
                    println!("Stopping tool service: {}...", tool_name);
                    let _ = Command::new(&tool_path).arg("uninstall").output();
                }
            }
        }
        // Brief settle time for services to release file handles
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    // Remove retired tools: check staging for .retired markers and delete installed binaries.
    // This handles tools that were replaced by embedded functionality (e.g., koi → koi-embedded).
    let staging_tools_dir = staging_bin_dir.join("tools");
    if staging_tools_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&staging_tools_dir)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            let marker_path = entry.path();
            if marker_path
                .extension()
                .map(|e| e == "retired")
                .unwrap_or(false)
            {
                let tool_name = marker_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                log_update(&format!("Retiring external tool: {}", tool_name));
                println!("  Retiring tool: {} (replaced by embedded)", tool_name);

                // Delete the installed binary
                let installed_exe = existing_tools_dir.join(format!("{}.exe", tool_name));
                if installed_exe.exists() {
                    match std::fs::remove_file(&installed_exe) {
                        Ok(_) => {
                            log_update(&format!(
                                "Removed retired tool binary: {:?}",
                                installed_exe
                            ));
                            println!("  ✓ {} removed", tool_name);
                        }
                        Err(e) => {
                            log_update(&format!(
                                "Failed to remove retired tool {}: {}",
                                tool_name, e
                            ));
                            eprintln!("  ⚠ {} removal failed: {}", tool_name, e);
                        }
                    }
                }

                // Remove the marker itself so it doesn't linger
                let _ = std::fs::remove_file(&marker_path);
            }
        }
    }

    // Copy all staged binaries to install directory
    println!("Installing staged binaries...");
    log_update("Copying staged binaries to install directory...");

    let mut installed_count = 0;
    if let Ok(entries) = std::fs::read_dir(&staging_bin_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let src_path = entry.path();
            let file_name = entry.file_name();

            if src_path.is_file() {
                // Top-level binaries go to exe_dir
                let dest_path = exe_dir.join(&file_name);

                log_update(&format!("Copying {:?} -> {:?}", src_path, dest_path));
                match std::fs::copy(&src_path, &dest_path) {
                    Ok(_) => {
                        installed_count += 1;
                        println!("  ✓ {}", file_name.to_string_lossy());
                    }
                    Err(e) => {
                        log_update(&format!("ERROR copying {:?}: {}", file_name, e));
                        eprintln!("  ✗ {} - {}", file_name.to_string_lossy(), e);
                    }
                }
            } else if src_path.is_dir() {
                // All subdirectories go to data_dir (e.g., .zen-garden/Companions)
                let data_dir_str = garden_common::constants::paths::data_dir();
                let subdir_dest = Path::new(&data_dir_str).join(&file_name);
                log_update(&format!(
                    "Copying subdir {:?} -> {:?}",
                    src_path, subdir_dest
                ));

                // Recursively copy directory
                fn copy_dir_recursive(
                    src: &Path,
                    dest: &Path,
                    log_fn: &dyn Fn(&str),
                ) -> std::io::Result<u32> {
                    let mut count = 0;
                    if !dest.exists() {
                        std::fs::create_dir_all(dest)?;
                    }
                    for entry in std::fs::read_dir(src)? {
                        let entry = entry?;
                        let src_path = entry.path();
                        let dest_path = dest.join(entry.file_name());
                        if src_path.is_dir() {
                            count += copy_dir_recursive(&src_path, &dest_path, log_fn)?;
                        } else {
                            log_fn(&format!("Copying {:?} -> {:?}", src_path, dest_path));
                            std::fs::copy(&src_path, &dest_path)?;
                            count += 1;
                        }
                    }
                    Ok(count)
                }

                match copy_dir_recursive(&src_path, &subdir_dest, &|msg| log_update(msg)) {
                    Ok(count) => {
                        installed_count += count;
                        println!("  ✓ {}/ ({} files)", file_name.to_string_lossy(), count);
                    }
                    Err(e) => {
                        log_update(&format!(
                            "ERROR copying {}: {}",
                            file_name.to_string_lossy(),
                            e
                        ));
                        eprintln!("  ✗ {}/ - {}", file_name.to_string_lossy(), e);
                    }
                }
            }
        }
    }

    log_update(&format!("Installed {} binaries", installed_count));
    println!("✓ Installed {} binaries", installed_count);

    // Post-copy: install external tools from the *installed* path (not staging).
    // The service registers its binary path, so it must point to the permanent location.
    // Convention: each .exe in tools/ supports `{exe} install` for service registration + start.
    let installed_tools_dir = Path::new(&garden_common::constants::paths::data_dir()).join("tools");
    if installed_tools_dir.exists() {
        log_update(&format!(
            "Installing external tools from: {:?}",
            installed_tools_dir
        ));
        println!("Installing external tools...");

        let mut tools_ok: u32 = 0;
        let mut tools_err: u32 = 0;

        if let Ok(entries) = std::fs::read_dir(&installed_tools_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let tool_path = entry.path();
                if tool_path.is_file() && tool_path.extension().map(|e| e == "exe").unwrap_or(false)
                {
                    let tool_name = tool_path.file_stem().unwrap_or_default().to_string_lossy();
                    log_update(&format!(
                        "Installing external tool: {} ({:?})",
                        tool_name, tool_path
                    ));
                    println!("  Installing tool: {}...", tool_name);

                    match Command::new(&tool_path).arg("install").output() {
                        Ok(output) if output.status.success() => {
                            log_update(&format!("Tool {} installed successfully", tool_name));
                            println!("  ✓ {} installed", tool_name);
                            tools_ok += 1;
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            log_update(&format!(
                                "Tool {} install returned non-zero: {} {}",
                                tool_name, stdout, stderr
                            ));
                            eprintln!(
                                "  ⚠ {} install: {} {}",
                                tool_name,
                                stdout.trim(),
                                stderr.trim()
                            );
                            tools_err += 1;
                        }
                        Err(e) => {
                            log_update(&format!("Failed to run {} install: {}", tool_name, e));
                            eprintln!("  ✗ {} - {}", tool_name, e);
                            tools_err += 1;
                        }
                    }
                }
            }
        }

        if tools_err == 0 {
            println!("✓ External tools: {} installed", tools_ok);
        } else {
            println!(
                "⚠ External tools: {} installed, {} failed",
                tools_ok, tools_err
            );
        }
        log_update(&format!(
            "External tools: {} installed, {} failed",
            tools_ok, tools_err
        ));
    }

    // Check if running as service
    let is_service = std::env::var("RUNNING_AS_SERVICE").is_ok();
    log_update(&format!("Running as service: {}", is_service));

    if is_service {
        println!("Starting Moss service...");
        log_update("Starting ZenGardenMoss service...");
        let output = Command::new("sc")
            .args(["start", "ZenGardenMoss"])
            .output()?;
        log_update(&format!(
            "Service start output: {:?}",
            String::from_utf8_lossy(&output.stdout)
        ));
        println!("✓ Service start triggered");
    } else {
        // Wait for port 7185 to become available (up to 10 seconds)
        println!(
            "Waiting for port {} to become available...",
            garden_common::constants::MOSS_HTTP
        );
        log_update(&format!(
            "Checking port {} availability...",
            garden_common::constants::MOSS_HTTP
        ));

        let port = garden_common::constants::MOSS_HTTP;
        for attempt in 1..=20 {
            match std::net::TcpListener::bind(format!("0.0.0.0:{}", port)) {
                Ok(listener) => {
                    drop(listener); // Release the port immediately
                    log_update(&format!(
                        "Port {} available after attempt {}",
                        port, attempt
                    ));
                    break;
                }
                Err(_) => {
                    if attempt == 20 {
                        log_update(&format!(
                            "WARNING: Port {} still in use after 10s, launching anyway",
                            port
                        ));
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }

        let target_exe = exe_dir.join("garden-moss.exe");
        println!("Launching new Moss...");
        log_update(&format!(
            "Launching new Moss: {:?} --cleanup-updater",
            target_exe
        ));
        let child = Command::new(&target_exe).arg("--cleanup-updater").spawn()?;
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

    log_update("=== cleanup_after_service_update: STARTED ===");

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    let temp_exe = exe_dir.join("garden-moss-temp.exe");

    log_update(&format!("Looking for temp updater: {:?}", temp_exe));

    if temp_exe.exists() {
        log_update("Temp updater file found, waiting for process to exit...");

        // Wait for garden-moss-temp.exe process to exit
        for attempt in 1..=20 {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq garden-moss-temp.exe"])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.contains("garden-moss-temp.exe") {
                log_update(&format!(
                    "Temp updater process exited after attempt {}",
                    attempt
                ));
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Remove temp binary
        match std::fs::remove_file(&temp_exe) {
            Ok(_) => log_update("Temp updater file removed successfully"),
            Err(e) => log_update(&format!("Failed to remove temp updater: {}", e)),
        }
    } else {
        log_update("No temp updater file found (already cleaned up or not an update)");
    }

    log_update("=== cleanup_after_service_update: COMPLETE ===");

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

    let exe_dir = current_exe
        .parent()
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
