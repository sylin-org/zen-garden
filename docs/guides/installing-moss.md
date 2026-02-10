---
audience: [operator]
doc_type: guide
status: current
last_verified: 2026-02-09
---

# Installing Moss

**Set up a Zen Garden stone on any machine with a single command.**

---

## What You'll Need

- A Linux (x64/x86) or Windows (x64) machine with an OS already running
- The `garden-moss` binary for your platform
- Root (Linux) or Administrator (Windows) access
- Optionally: the matching platform package for offline install

## Overview

There are three ways to install Moss, depending on your connectivity:

| Method | What you provide | Internet required? |
|--------|-----------------|-------------------|
| **Offline** | `garden-moss` + platform package in the same directory | No |
| **USB** | Files on a USB drive, run from there | No |
| **Online** | `garden-moss` binary only | Yes (future) |

All three produce the same result: Moss running as a system service.

---

## Step 1: Get the Files

### Option A: Offline install (recommended)

Download the binary and matching platform package from the latest release and place them in the same directory:

**Linux x64:**

```
garden-moss
zen-garden-0.1.202602091234-linux-x64.tar.gz
```

**Windows x64:**

```
garden-moss.exe
zen-garden-0.1.202602091234-windows-x64.zip
```

### Option B: USB install

Place the binary and package on a USB drive. Moss detects removable media automatically — it copies everything to the permanent install location before proceeding.

### Option C: Online install (future)

When GitHub release checking is implemented, you can provide just the binary. Moss downloads the latest matching package automatically.

---

## Step 2: Run the Installer

### Linux

```bash
sudo ./garden-moss install
```

### Windows

Open an Administrator terminal (right-click, "Run as administrator") and run:

```powershell
.\garden-moss.exe install
```

### What happens

The installer:

1. **Resolves the package** — finds the sibling `.tar.gz` / `.zip` in the current directory
2. **Creates directories** — `/var/lib/zen-garden` and `/etc/zen-garden` (Linux) or `C:\ProgramData\ZenGarden` (Windows)
3. **Installs binaries** — `garden-moss`, `garden-rake`, `garden-lantern`, and companion scripts
4. **Registers the service** — systemd unit (Linux) or Windows Service (Windows) with auto-start
5. **Starts Moss** and verifies the health endpoint responds
6. **Prints a summary** with the API URL and management commands

Example output (Linux):

```
  Zen Garden Moss Installer
  0.1.202602091234

Resolving package...
  Checking for zen-garden-*-linux-x64.tar.gz...
  Found: zen-garden-0.1.202602091234-linux-x64.tar.gz

Installing Zen Garden...
  Creating directories...
    /var/lib/zen-garden/
    /etc/zen-garden/
  Installing binaries...
    garden-moss       -> /usr/local/bin/garden-moss
    garden-rake       -> /usr/local/bin/garden-rake
    garden-lantern    -> /usr/local/bin/garden-lantern
  Installing scripts...
    moss-update-helper.sh -> /usr/local/bin/moss-update-helper.sh
  Installing service...
    garden-moss.service -> /etc/systemd/system/garden-moss.service
    systemctl daemon-reload... done.
    Service enabled (start on boot)

Starting Moss... started.

Checking health... healthy.

Zen Garden is ready.

  API:     http://localhost:7185
  Health:  http://localhost:7185/health
  CLI:     garden-rake status

  Manage the service:
    systemctl status garden-moss      View status
    systemctl stop garden-moss        Stop
    systemctl restart garden-moss     Restart
    journalctl -u garden-moss -f      Follow logs
```

---

## Step 3: Verify the Installation

### Check the health endpoint

```bash
curl http://localhost:7185/health
```

### Check service status

**Linux:**

```bash
systemctl status garden-moss
```

**Windows:**

```powershell
sc query ZenGardenMoss
```

### Use the CLI

```bash
garden-rake status
```

---

## Upgrading

Run `garden-moss install` again with a newer package. The installer handles upgrades automatically:

- **Linux**: Stops the service, deploys new binaries, restarts
- **Windows**: Stops the service, deletes the old SCM entry, waits for purge, recreates with the new binary, restarts

Data and configuration are preserved across upgrades.

---

## Uninstalling

### Linux

```bash
sudo garden-moss uninstall
```

### Windows

Run in an Administrator terminal:

```powershell
.\garden-moss.exe uninstall
```

### What gets removed

- Service registration (systemd unit / Windows Service)
- Binaries (`garden-moss`, `garden-rake`, `garden-lantern`)
- Helper scripts (`moss-update-helper.sh`, `garden-upgrade.sh`)
- Firewall rules (Windows)

### What gets preserved

- **Data**: `/var/lib/zen-garden/` (Linux) or `C:\ProgramData\ZenGarden\.zen-garden\` (Windows)
- **Configuration**: `/etc/zen-garden/` (Linux)

To remove everything including data:

```bash
# Linux
sudo rm -rf /var/lib/zen-garden /etc/zen-garden
```

---

## USB / Removable Media

When you run `garden-moss install` from a USB drive, Moss:

1. Detects the removable media
2. Copies the binary and sibling package to the permanent install location
3. Installs from the local copy
4. Cleans up temporary files

This ensures the service never points to a drive that could be ejected.

---

## Troubleshooting

### "requires root" / "requires Administrator privileges"

Install and uninstall need elevated permissions to register system services.

**Linux**: Prefix the command with `sudo`.

**Windows**: Right-click your terminal and choose "Run as administrator".

### "No package found"

Moss could not find a matching platform package in the current directory. Make sure the `.tar.gz` (Linux) or `.zip` (Windows) file is in the same directory as the `garden-moss` binary.

The package file must match the pattern: `zen-garden-*-{platform}.{ext}`

### Health check shows "not yet responding"

This is normal during first boot. Moss takes a few seconds to initialize Docker connections, run first-boot discovery, and start background tasks. Wait 15-30 seconds and check again:

```bash
curl http://localhost:7185/health
```

### Service fails to start (Windows)

Check the Windows Event Viewer under **Windows Logs > System** for entries from `ZenGardenMoss`. Common causes:

- Port 7185 already in use by another process
- Docker Desktop not running
- Missing `C:\ProgramData\ZenGarden\garden-moss.exe`

### Service fails to start (Linux)

Check the journal:

```bash
journalctl -u garden-moss -n 50 --no-pager
```

Common causes:

- Port 7185 already in use
- Docker not installed or not running (`systemctl status docker`)
- Missing binaries in `/usr/local/bin/`

---

## Comparison: Install vs. USB (NewStone)

| Feature | `garden-moss install` | NewStone USB |
|---------|----------------------|-------------|
| Requires existing OS | Yes | No (installs Ubuntu) |
| Internet required | No (offline) | No |
| Creates `stone` user | No | Yes |
| Installs Docker | No | Yes |
| Configures networking | No | Yes |
| Single-machine setup | Best choice | Overkill |
| Fleet provisioning | Not designed for | Best choice |
| Air-gapped bare metal | Not applicable | Best choice |

Use `garden-moss install` when you already have a machine with an OS running. Use the NewStone USB when you're provisioning bare metal from scratch.

---

## Next Steps

- [Your First Stone](first-stone.md) — full walkthrough including USB provisioning
- [Troubleshooting](troubleshooting.md) — common issues and solutions
- [Nurturing](nurturing.md) — keeping your stones updated
