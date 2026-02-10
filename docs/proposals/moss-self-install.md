---
audience: [contributor, operator]
doc_type: proposal
status: draft
last_verified: 2026-02-09
---

# Moss Self-Install

**Author**: Collaborative design
**Date**: 2026-02-09

---

## Problem Statement

Today, creating a Zen Garden stone requires either a USB installer (NewStone) or manual SSH-based deployment. Both assume a dedicated provisioning workflow. There is no way for a user to simply download a single binary and run it on an existing machine — whether that's a Linux server in a closet or a Windows desktop.

This creates unnecessary friction for:

- **Windows users** who have no USB installer equivalent at all
- **Linux users** who already have Debian/Ubuntu running and just want to add a stone
- **Developers** who want to try Zen Garden on their own machine
- **Air-gapped environments** where internet isn't available but a binary + package file can be carried in

Meanwhile, stones have no way to check for newer Moss versions. Offering and firmware updates are detected, but the daemon itself is a blind spot — updates require manual redeployment via `deploy.ps1` or SSH.

## Proposed Solution

### One binary, one command

```
garden-moss install
```

That's the entire installation experience. Everything else — directories, service registration, companion binaries, configuration — happens automatically.

### Three installation tiers

| Tier | What the user provides | What happens |
|------|----------------------|--------------|
| **Online** | `garden-moss` binary | Moss downloads the latest matching package from GitHub Releases, extracts it, installs everything, registers as a service |
| **Offline** | `garden-moss` binary + `zen-garden-*-{platform}.tar.gz/.zip` in the same directory | Moss finds the sibling package, skips the download, installs from it |
| **USB** | NewStone USB stick | Unchanged — preseed handles everything, Moss is pre-installed |

All three tiers produce the same end state: a fully provisioned stone with Moss running as a system service.

### Critical constraint: install does not start the daemon

The `install` and `uninstall` subcommands are **pure setup/teardown operations**. They must never activate the daemon loop, API server, Docker orchestration, or any service stack. They run to completion and exit.

The daemon starts only when the system service manager (systemd/Windows SCM) starts it — after installation is complete, on boot, or via `sc start`/`systemctl start`.

```
garden-moss install     → setup only, exits when done
garden-moss uninstall   → teardown only, exits when done
garden-moss             → daemon mode (started by service manager)
```

This separation ensures:
- No half-initialized daemon competing with service manager
- Clean error reporting during install (stdout, not logs)
- Uninstall can stop the service cleanly before removing files
- The user sees the install complete before the daemon takes over

### What `install` does

#### Phase 1: Resolve package

```
Resolving package...
  Checking current directory for zen-garden-*-linux-x64.tar.gz...
  Found: zen-garden-0.1.202602091234-linux-x64.tar.gz
```

or:

```
Resolving package...
  Checking current directory for zen-garden-*-linux-x64.tar.gz...
  No local package found
  Fetching latest release from GitHub...
  Downloading zen-garden-0.1.202602091234-linux-x64.tar.gz (14.2 MB)...
  ████████████████████████████████████████ 100%
  Verified SHA256: a1b2c3d4...
```

or:

```
Resolving package...
  Checking current directory for zen-garden-*-linux-x64.tar.gz...
  No local package found
  No internet connectivity detected

  To install offline, place the platform package in the same directory:
    zen-garden-{version}-linux-x64.tar.gz

  Download from: https://github.com/{owner}/{repo}/releases/latest
```

Package resolution order:
1. Local sibling file matching `zen-garden-*-{platform}.{ext}`
2. GitHub Releases API → latest release → matching asset
3. Graceful failure with clear instructions

#### Phase 2: Extract and install

##### Linux

```
Installing Zen Garden...
  Creating directories...
    /var/lib/zen-garden/
    /etc/zen-garden/
  Installing binaries...
    garden-moss       → /usr/local/bin/garden-moss
    garden-rake       → /usr/local/bin/garden-rake
    garden-lantern    → /usr/local/bin/garden-lantern
  Installing scripts...
    moss-update-helper.sh → /usr/local/bin/moss-update-helper.sh
    garden-upgrade.sh     → /usr/local/bin/garden-upgrade.sh
  Installing service...
    garden-moss.service   → /etc/systemd/system/garden-moss.service
    systemctl daemon-reload
    systemctl enable garden-moss
  Writing default configuration...
    /etc/zen-garden/garden-moss.toml
```

##### Windows

```
Installing Zen Garden...
  Creating directories...
    C:\ProgramData\ZenGarden\
    C:\ProgramData\ZenGarden\.zen-garden\
  Installing binaries...
    garden-moss.exe   → C:\ProgramData\ZenGarden\garden-moss.exe
    garden-rake.exe   → C:\ProgramData\ZenGarden\garden-rake.exe
    garden-lantern.exe→ C:\ProgramData\ZenGarden\garden-lantern.exe
  Registering Windows service...
    Service: ZenGardenMoss (auto-start)
```

#### Phase 3: Start

```
Starting Moss...
  systemctl start garden-moss

Zen Garden is ready.

  Stone name:    stone-quiet-morning
  API:           http://localhost:7185
  Health:        http://localhost:7185/health
  CLI:           garden-rake status

  Manage the service:
    systemctl status garden-moss      View status
    systemctl stop garden-moss        Stop
    systemctl restart garden-moss     Restart
    journalctl -u garden-moss -f      Follow logs
```

The start phase issues the service manager command and waits briefly for the health endpoint to respond. If the health check passes, it prints the success summary. If it doesn't respond within a few seconds, it still prints the summary but notes that startup may take a moment (Docker initialization, first-boot discovery, etc.).

### What `uninstall` does

```
garden-moss uninstall
```

```
Uninstalling Zen Garden...
  Stopping service...
    systemctl stop garden-moss
  Disabling service...
    systemctl disable garden-moss
  Removing service file...
    /etc/systemd/system/garden-moss.service
    systemctl daemon-reload
  Removing binaries...
    /usr/local/bin/garden-moss
    /usr/local/bin/garden-rake
    /usr/local/bin/garden-lantern
    /usr/local/bin/moss-update-helper.sh
    /usr/local/bin/garden-upgrade.sh

Zen Garden has been removed.

  Data preserved at: /var/lib/zen-garden/
  Config preserved at: /etc/zen-garden/

  To remove all data: rm -rf /var/lib/zen-garden /etc/zen-garden
```

Data and configuration are **never** deleted automatically. The user must explicitly remove them. This prevents accidental loss of offering data, seed banks, and configuration.

### GitHub release checking (self-nourishment)

Moss periodically checks GitHub Releases for newer versions of itself. This integrates into the existing nourishment system as a new update source alongside Docker registry (offerings) and fwupd (firmware).

#### Detection

On the nourishment check cycle, Moss queries:

```
GET https://api.github.com/repos/{owner}/{repo}/releases/latest
```

The release tag contains the version (e.g., `v0.1.202602091234`). Moss compares against its own compiled-in version (`cli::VERSION`). If newer, it surfaces a `Moss` update in the nourishment response.

The release must contain a platform-matching asset:
- `zen-garden-{version}-linux-x64.tar.gz`
- `zen-garden-{version}-linux-x86.tar.gz`
- `zen-garden-{version}-windows-x64.zip`

If the running platform's asset isn't in the release, no update is surfaced.

#### Rake integration

```
$ garden-rake nourish

  Zen Garden Nourishment Check
  ────────────────────────────────────

  stone-quiet-morning
    Moss       0.1.202601151234 → 0.1.202602091234
    MongoDB    7.0.14 → 7.0.16

  stone-mossy-brook
    Moss       0.1.202601151234 → 0.1.202602091234

  3 updates available (2 moss, 1 offering)

  [A] All updates  [O] Offerings only  [M] Moss only  [F] Firmware only  [Q] Cancel
```

#### Execution

When the user selects Moss updates, each stone:

1. Downloads the matching package asset from the GitHub release
2. Validates SHA256 against the manifest
3. Extracts to `/var/lib/zen-garden/staging/validated/`
4. Triggers service restart

The existing `moss-update-helper.sh` (ExecStartPre) handles the actual binary replacement on Linux. On Windows, the existing `spawn_windows_updater` / `finalize_service_update` flow handles it.

No new update mechanism is needed — the staged upgrade pipeline already exists for both platforms. The only new piece is the GitHub release as a package source.

#### Rate limiting and caching

- Check frequency: once per hour (configurable via `garden-moss.toml`)
- Cache the latest known version in `{data_dir}/latest-release.json`
- Respect GitHub API rate limits (60 req/hr unauthenticated, 5000/hr with token)
- Optional `github_token` in config for private repos or higher rate limits
- ETag-based conditional requests to minimize API usage

### Relationship to existing deployment methods

| Method | Still used for | Changed? |
|--------|---------------|----------|
| **NewStone USB** | Bare metal provisioning, fleet deployment, air-gapped installs | No change |
| **deploy.ps1** | Network-wide deployment from dev machine | No change |
| **`garden-moss install`** | New single-machine setup | **New** |
| **`garden-rake nourish`** | Ongoing updates | **Extended** with Moss self-update |

The USB installer doesn't go away. It's still the right tool for headless machines, bulk provisioning, and air-gapped environments where you're installing the OS itself. `garden-moss install` is for machines that already have an OS running.

## Alternatives Considered

### cargo install garden-moss

- **Pros**: Familiar to Rust developers, leverages crates.io infrastructure
- **Cons**: Requires Rust toolchain on every stone, path dependencies block crates.io publishing (garden-common, garden-build-utils must be published first), only delivers the binary — no service registration, no companion binaries, no scripts
- **Why not**: Too narrow. Only serves Rust developers. Still needs a post-install bootstrap step. The `install` subcommand approach works for everyone regardless of how they got the binary.

### Platform-specific installers (.deb, .msi)

- **Pros**: Native package management, familiar `apt install` / double-click workflow
- **Cons**: Must build and maintain separate packaging pipelines per platform, signing certificates needed, repository hosting needed, harder to iterate quickly
- **Why not**: High maintenance overhead for a small project. The single-binary approach achieves the same UX with zero packaging infrastructure. Can always add .deb/.msi later as a layer on top — the install logic lives in the binary either way.

### Download script (curl | bash)

- **Pros**: Zero-friction for Linux users, common pattern
- **Cons**: Security concerns (pipe to shell), another artifact to maintain, platform detection logic duplicated outside Rust, can't work offline
- **Why not**: The binary itself is the installer. Downloading the binary and running `install` is the same number of steps as `curl | bash` but without the security baggage. A one-liner download instruction in the README achieves the same effect: `curl -L {url} -o garden-moss && chmod +x garden-moss && sudo ./garden-moss install`

## Impact

**What gets easier:**
- First-time setup on any machine with an OS already running
- Windows stones become a real thing (no USB installer needed)
- Moss self-updates — no more manual `deploy.ps1` for version bumps
- Offline deployment with just two files (binary + package)

**What changes:**
- `cli.rs`: New `Install` and `Uninstall` subcommands (cross-platform)
- `main.rs`: Early exit for install/uninstall (no daemon startup)
- Nourishment system: New `Moss` update source alongside offerings/firmware
- `nourishment.rs` (common): New `Update::Moss` variant
- `nourishment.rs` (API): GitHub release checking logic
- `nourish.rs` (rake): Display and execution for Moss updates
- Existing Windows `TakeRoot`/`InstallService`: Replaced by unified `Install`

**What breaks:**
- Nothing. All existing workflows continue unchanged. `TakeRoot` can remain as a hidden alias for backwards compatibility.

## Open Questions

- **GitHub repository visibility**: If the repo is private, the release API requires authentication. Should `install` support a `--github-token` flag, or rely solely on the config file?
- **Stone name generation**: On first install, should `install` prompt for a stone name interactively, generate one automatically (current behavior), or accept `--stone-name` as a flag?
- **Docker prerequisite**: Should `install` check for Docker and offer to install it (like `setup-stone.sh` does), or just warn and let the user handle it?
- **Linux user creation**: Should `install` create the `stone` user if it doesn't exist, or is that only for USB-provisioned stones?

## Reference Implementation: Koi

The [Koi mDNS daemon](https://github.com/sylin-org/koi) implements a production-grade `install`/`uninstall` system across Windows, Linux, and macOS. This section documents the patterns worth adopting and the caveats to avoid.

**Source files**: `koi/src/platform/windows.rs`, `koi/src/platform/unix.rs`, `koi/src/platform/macos.rs`, `koi/src/main.rs`, `koi/src/config.rs`

### Architecture: synchronous subcommands before async runtime

Koi's most important design choice: `install` and `uninstall` run **synchronously in `main()`** before the Tokio runtime is created. This means they can never accidentally spin up the daemon loop.

```rust
// koi/src/main.rs:53-101
fn main() -> anyhow::Result<()> {
    // Windows SCM dispatch happens first (blocks if launched by SCM)
    #[cfg(windows)]
    if platform::windows::try_run_as_service() {
        return Ok(());
    }

    let cli = Cli::parse();

    // Synchronous subcommands — no Tokio runtime, no daemon
    if let Some(command) = &cli.command {
        match command {
            Command::Install => return platform::install(),
            Command::Uninstall => return platform::uninstall(),
            _ => {}
        }
    }

    // Everything below needs a Tokio runtime
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_main(cli))
}
```

**Adopt for Moss**: The current Moss `main.rs` creates the Tokio runtime via `#[tokio::main]` before dispatching subcommands. Restructure to match Koi's pattern: parse CLI, dispatch synchronous commands (install/uninstall) with early return, then create the runtime for daemon mode.

### Privilege escalation: fail fast with clear hints

Both platforms check elevation before making any changes. No partial state if unprivileged.

```rust
// koi/src/platform/windows.rs:550-570
fn ensure_elevated(verb: &str) -> anyhow::Result<()> {
    // `net session` succeeds only in an elevated context.
    let ok = Command::new("net")
        .arg("session")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok { Ok(()) }
    else {
        anyhow::bail!(
            "garden-moss {verb} requires Administrator privileges — \
             right-click your terminal and choose \"Run as administrator\""
        );
    }
}

// koi/src/platform/unix.rs:187-193
fn check_root(verb: &str) -> anyhow::Result<()> {
    let output = Command::new("id").arg("-u").output();
    match output {
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "0" => Ok(()),
        _ => anyhow::bail!(
            "garden-moss {verb} requires root — try: sudo garden-moss {verb}"
        ),
    }
}
```

**Adopt for Moss**: Use the same pattern. Moss's current `install_windows_service()` doesn't check elevation — it just fails cryptically when `sc create` returns ACCESS_DENIED.

### Idempotent upgrade: stop, delete, wait, recreate

Koi's Windows installer handles both fresh installs and upgrades in a single code path. The key insight is the `wait_for_delete` polling loop — the Windows SCM defers service removal, and attempting to recreate before the old entry is purged fails with a confusing error.

```rust
// koi/src/platform/windows.rs:59-110 (condensed)
let service = match manager.open_service(SERVICE_NAME, access) {
    Ok(existing) => {
        // Upgrade path: stop, delete, wait for purge, recreate
        if status.current_state != ServiceState::Stopped {
            existing.stop();
            wait_for_stop(&existing)?;  // poll 500ms, timeout 30s
        }
        existing.delete()?;
        drop(existing);
        wait_for_delete(&manager)?;  // poll until SCM purges entry
        manager.create_service(&info, access)?  // recreate with new config
    }
    Err(e) if e == ERROR_SERVICE_NOT_FOUND => {
        // Fresh install
        manager.create_service(&info, access)?
    }
    Err(e) => return Err(e.into()),
};
```

The polling helpers:

```rust
// koi/src/platform/windows.rs:574-607
fn wait_for_stop(service: &Service) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        match service.query_status() {
            Ok(s) if s.current_state == ServiceState::Stopped => return Ok(()),
            Ok(_) if Instant::now() >= deadline => {
                anyhow::bail!("Service did not stop within 30s");
            }
            Ok(_) => continue,
            Err(e) => anyhow::bail!("Could not query service status: {e}"),
        }
    }
}

fn wait_for_delete(manager: &ServiceManager) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Err(_) => return Ok(()),  // entry purged
            Ok(_) if Instant::now() >= deadline => {
                anyhow::bail!("Old service entry not purged within 30s");
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}
```

**Adopt for Moss**: Moss's current `install_windows_service()` checks if the service exists and bails with "Service already installed". It should instead adopt Koi's stop-delete-wait-recreate pattern so that `garden-moss install` works as both first install and upgrade.

### Recovery policy

Koi configures the Windows service to auto-restart on failure with escalating delays:

```rust
// koi/src/platform/windows.rs:117-146
ServiceFailureActions {
    reset_period: After(Duration::from_secs(86_400)),  // reset after 24h clean
    actions: Some(vec![
        ServiceAction { action_type: Restart, delay: Duration::from_secs(5) },
        ServiceAction { action_type: Restart, delay: Duration::from_secs(10) },
        ServiceAction { action_type: None, delay: Duration::ZERO },  // give up
    ]),
}
// Also: trigger recovery on non-crash failures (non-zero exit)
service.set_failure_actions_on_non_crash_failures(true);
```

**Adopt for Moss**: Moss should use the same recovery policy. Currently Moss's Windows service has no recovery configuration — if it crashes, it stays dead until someone manually restarts it.

### Best-effort non-critical steps

Koi never aborts installation over non-critical failures. Directory creation, firewall rules, service description, and recovery policy are all wrapped in non-fatal paths:

```rust
// koi/src/platform/windows.rs:148-171
// Log directory — warn but continue
match std::fs::create_dir_all(&log_dir) {
    Ok(()) => println!("  Log directory: {}", log_dir.display()),
    Err(e) => println!("  Warning: could not create log directory: {e}"),
}

// Firewall rules — never abort install
let fw = create_firewall_rule(FIREWALL_RULE, "UDP", PORT, &exe_path);
if !fw {
    println!("  Warning: could not set firewall rule for UDP {PORT}");
}
```

**Adopt for Moss**: Apply the same pattern. Service registration is critical (fail the install). Firewall rules, log directories, and configuration defaults are non-critical (warn and continue).

### Firewall rules: delete-then-create for idempotency

```rust
// koi/src/platform/windows.rs:467-487
fn create_firewall_rule(name: &str, protocol: &str, port: u16, exe: &Path) -> bool {
    // Delete first (ignore errors — rule may not exist)
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule"])
        .arg(format!("name={name}"))
        .output();

    // Then create
    let result = Command::new("netsh")
        .args(["advfirewall", "firewall", "add", "rule"])
        .arg(format!("name={name}"))
        .args(["dir=in", "action=allow"])
        .arg(format!("protocol={protocol}"))
        .arg(format!("localport={port}"))
        .arg(format!("program={}", exe.display()))
        .output();

    matches!(result, Ok(output) if output.status.success())
}
```

**Adopt for Moss**: Moss needs firewall rules for port 7185 (HTTP API) and 7184 (mDNS/UDP). Use the same delete-then-create pattern. On uninstall, remove the rules.

### Linux: binary copy + unit file generation

Koi copies the current executable to `/usr/local/bin/` and generates the systemd unit file in code:

```rust
// koi/src/platform/unix.rs:48-104 (condensed)
// Copy binary
std::fs::copy(&exe_path, &install_path)?;
let perms = std::fs::Permissions::from_mode(0o755);
std::fs::set_permissions(&install_path, perms)?;

// Generate and write unit file
let unit_contents = generate_unit_file(&install_path);
std::fs::write(&unit_path, unit_contents)?;

// Reload + enable + start
Command::new("systemctl").args(["daemon-reload"]).output();
Command::new("systemctl").args(["enable", SERVICE_NAME]).output();
Command::new("systemctl").args(["start", SERVICE_NAME]).output();
```

**Adapt for Moss**: Moss's case is more complex because it installs a multi-binary package (moss, rake, lantern, scripts), not just one binary. But the systemd integration pattern is identical. Moss already has a `garden-moss.service` template in `installer/package-assets/` — the install command should embed or generate the same content.

### systemd `Type=notify`

Koi uses `Type=notify` so systemd knows exactly when the daemon is ready:

```rust
// koi/src/platform/unix.rs:3-13
pub fn notify_ready() -> anyhow::Result<()> {
    if let Ok(socket_path) = std::env::var("NOTIFY_SOCKET") {
        use std::os::unix::net::UnixDatagram;
        let socket = UnixDatagram::unbound()?;
        socket.send_to(b"READY=1", &socket_path)?;
    }
    Ok(())
}
```

**Consider for Moss**: Moss currently uses `Type=simple`. Switching to `Type=notify` would let systemd accurately report readiness and prevent dependent services from starting too early. Separate enhancement but worth noting.

### Removable media detection

Koi's existing Moss service installer already detects execution from removable media (USB drives) and copies the binary to a permanent location before registering the service. This is critical — a service registered with a path on a USB drive breaks as soon as the drive is removed.

```rust
// moss/src/infra/service.rs:103-125 (existing Moss code)
let is_removable = crate::infra::is_running_from_removable_media(&current_exe)?;
let install_exe = if is_removable {
    let install_dir = PathBuf::from(r"C:\ProgramData\ZenGarden");
    std::fs::create_dir_all(&install_dir)?;
    let target_exe = install_dir.join("garden-moss.exe");
    std::fs::copy(&current_exe, &target_exe)?;
    target_exe
} else {
    current_exe
};
```

**Extend for Moss**: The self-install command must generalize this to copy not just the binary but also the sibling package to the permanent install location before extraction. When running from removable media:

1. Copy `garden-moss` binary to install directory (`/usr/local/bin/` or `C:\ProgramData\ZenGarden\`)
2. Copy the sibling package (if present) to a temp location on the same filesystem
3. Extract from the local copy, not from the removable media
4. Clean up the temp package copy after extraction

This ensures the install completes even if the user removes the USB drive mid-install, and the service is never registered with a path on removable media.

### Uninstall: idempotent and data-preserving

Koi's uninstall is safe to run multiple times and never deletes user data:

```rust
// koi/src/platform/unix.rs:123-182 (condensed)
pub fn uninstall() -> anyhow::Result<()> {
    check_root("uninstall")?;

    if unit_path.exists() {
        if systemctl_check("is-active") {
            Command::new("systemctl").args(["stop", SERVICE_NAME]).output();
        }
        Command::new("systemctl").args(["disable", SERVICE_NAME]).output();
        std::fs::remove_file(&unit_path)?;
        Command::new("systemctl").args(["daemon-reload"]).output();
    } else {
        println!("  Service not found, cleaning up remaining files...");
    }

    Ok(())
}
```

Windows uninstall additionally removes firewall rules and cleans up empty directories:

```rust
// koi/src/platform/windows.rs:272-282
// Log directory — remove only if empty
match std::fs::remove_dir(&log_dir) {
    Ok(()) => {}
    Err(e) if e.kind() == ErrorKind::NotFound => {}
    Err(_) => println!("  Logs preserved at: {}", log_dir.display()),
}
// Parent data directory — remove only if empty
let _ = std::fs::remove_dir(&data_dir);  // silent
```

**Adopt for Moss**: Same principle. Uninstall removes the service, binaries, and scripts. Data (`/var/lib/zen-garden/`) and config (`/etc/zen-garden/`) are preserved with a message telling the user how to remove them if desired.

### macOS launchd support

Koi implements macOS support via LaunchDaemons with a modern/legacy fallback chain:

```rust
// koi/src/platform/macos.rs:152-168
fn launchctl_bootstrap(plist_path: &Path) -> bool {
    // Try modern command first (macOS 10.11+)
    let result = Command::new("launchctl")
        .args(["bootstrap", "system", &plist_str])
        .output();
    if matches!(&result, Ok(o) if o.status.success()) {
        return true;
    }
    // Fall back to legacy command
    let result = Command::new("launchctl")
        .args(["load", "-w", &plist_str])
        .output();
    matches!(result, Ok(o) if o.status.success())
}
```

The plist sets `RunAtLoad: true` and `KeepAlive.SuccessfulExit: false` (restart on non-zero exit), with logs directed to `/var/log/koi.log`.

**Future reference for Moss**: Not in scope for V1 (Linux + Windows focus), but the pattern is documented here for when macOS support is needed. The three-platform abstraction (`platform::install()` dispatching to OS-specific modules) scales cleanly.

### Caveats to avoid

#### 1. No health check after start

Koi starts the service and prints success, but never verifies the daemon is actually running and healthy. If the binary crashes on startup, the user sees "Service started" followed by a silent failure.

**Fix for Moss**: After starting the service, poll the health endpoint (`http://localhost:7185/health`) for a few seconds. Print the success summary only after confirming the daemon is alive. If the health check fails, print a diagnostic message with log location and troubleshooting hints.

#### 2. No rollback on partial failure

If Koi's service creates but won't start, you're left with a broken installed service. There's no automatic rollback.

**Fix for Moss**: If the service fails to start after installation, offer clear recovery guidance. For upgrades specifically, the existing staged upgrade pipeline with `moss-update-helper.sh` provides implicit rollback — the old binaries remain in place until the pre-start hook atomically installs the new ones.

#### 3. Binary overwrite while running (Linux)

Koi stops the service, then copies the new binary over the old one with `std::fs::copy()`. This works on Linux (inode semantics — the old process keeps running from the old inode) but there's a brief window where a crash during the copy could leave a truncated binary.

**Fix for Moss**: Use the existing staged upgrade pattern. Copy the package contents to `staging/validated/` first, then restart the service. The `moss-update-helper.sh` pre-start hook handles the atomic move into `/usr/local/bin/`. This is safer than direct overwrite.

#### 4. No package concept

Koi only installs itself (one binary). There's no companion binary resolution, no manifest verification, no SHA256 checks. This is fine for a single-binary daemon but insufficient for Moss's multi-binary package.

**Fix for Moss**: The install command must resolve a full package (moss + rake + lantern + scripts + companions), verify the `package.json` manifest checksums, and install all components. The package format and verification already exist in the build system — reuse them.

#### 5. Koi preserves the binary on uninstall (Linux/macOS)

Koi's Linux uninstall removes the service but leaves `/usr/local/bin/koi` in place, just printing a note. This is arguably too conservative — a user running `uninstall` expects the software to be removed.

**Fix for Moss**: Remove all installed binaries and scripts during uninstall. Only preserve data and configuration (the user's stuff), not the software itself.

#### 6. Windows breadcrumb path mismatch

Koi writes the breadcrumb to `%LOCALAPPDATA%` (per-user), but the Windows service runs as LocalSystem (which has a different `LOCALAPPDATA`). This means a client running as the logged-in user may not find the daemon's breadcrumb.

**Not applicable to Moss**: Moss uses a well-known port (7185) and mDNS discovery, not breadcrumb files. No action needed.

#### 7. Service launch arguments

Koi registers the service with `launch_arguments: vec!["--daemon"]` to distinguish service mode from interactive mode. The `--daemon` flag suppresses interactive prompts and adjusts logging.

**Consider for Moss**: Moss doesn't currently need this distinction because it always runs as a daemon. But if install/uninstall subcommands are added, the service registration must not include `install` or `uninstall` in the launch arguments — just the bare binary path.

#### 8. No `windows-service` crate in Moss

Koi uses the `windows-service` crate (v0.8) for proper SCM integration — registering the control handler, reporting `StartPending`/`Running`/`StopPending`/`Stopped` states, and handling shutdown signals. Moss currently uses raw `sc.exe` commands for service creation and doesn't integrate with the SCM runtime protocol at all.

**Fix for Moss**: Add `windows-service` as a dependency and implement proper SCM integration. This gives:
- Correct service state reporting (systemd equivalent of `Type=notify`)
- Graceful shutdown on SCM stop/shutdown signals
- Recovery policy configuration via the API instead of `sc.exe`
- Service creation with proper type safety instead of string-based `sc.exe` argument formatting

## References

- [Nourishment Safe Updates](nourishment-safe-updates.md) — existing update detection and execution spec
- [Stone Lifecycle Operations](stone-lifecycle-operations.md) — stone state management
- [garden-moss.service](../../installer/package-assets/garden-moss.service) — systemd service unit
- [moss-update-helper.sh](../../installer/moss-update-helper.sh) — pre-start binary installer
- [setup-stone.sh](../../installer/setup-stone.sh) — existing Linux bootstrap script
- [service.rs](../../src/moss/src/infra/service.rs) — existing Windows service installer
- Koi install/uninstall: `koi/src/platform/windows.rs`, `koi/src/platform/unix.rs`, `koi/src/platform/macos.rs`
- Koi CLI dispatch: `koi/src/main.rs:53-101` — synchronous subcommand pattern
- Koi config/paths: `koi/src/config.rs` — service paths, breadcrumb system
