---
audience: [developer, contributor]
doc_type: decision
status: draft
last_verified: 2026-03-16
---

# BUILD-0003: Self-Deploying Moss

**Date**: 2026-03-16
**Status**: Draft
**Depends on**: [BUILD-0002 (Unified Deployment Packages)](BUILD-0002-unified-deployment-packages.md)
**Supersedes**: Portions of BUILD-0002 (platform-specific finalization via shell scripts)
**Applies to**: `moss` (installer module), `installer/` (build scripts, USB creator), `deploy.ps1`

## Context

Zen Garden deployment relied on a layered stack of shell scripts, filesystem mirrors, and platform-specific helpers:

| Artifact | Role | Language |
|----------|------|----------|
| `setup-stone.sh` | Full OS provisioning on fresh Debian | Bash |
| `moss-update-helper.sh` | ExecStartPre: deploy staged binaries before Moss starts | Bash |
| `garden-upgrade.sh` | Legacy upgrade helper | Bash (deprecated) |
| `garden.conf` | Timezone/NTP configuration sourced by shell scripts | Bash source |
| `stone-root/` | Filesystem mirror tree copied to target by preseed | Directory structure |
| `package-assets/garden-moss.service` | Systemd unit file shipped in package | INI |
| `installer/templates/*.template` | Preseed, service, config templates | Mixed |

The `garden-moss install` command (BUILD-0002) handled binary deployment and service registration but delegated OS provisioning to `setup-stone.sh` and pre-start staging to `moss-update-helper.sh`. This created three problems:

1. **Duplication**: Binary deployment logic existed in Rust (`linux.rs`), Bash (`setup-stone.sh`, `moss-update-helper.sh`), and PowerShell (`deploy.ps1`). Bug fixes in one path did not reach the others.

2. **Incomplete install**: `garden-moss install` did not create the `stone` user, install Docker, configure DNS resolution, create a default config file, or start the service on Linux. A user running `garden-moss install` on a bare Debian machine got a partially configured stone.

3. **Scaffolding overhead**: The package carried a `scripts/` directory with filesystem-mirrored paths (`scripts/etc/systemd/system/garden-moss.service` → `/etc/systemd/system/garden-moss.service`). The build system assembled this tree. The install command walked it. The update helper walked it again. All three implementations needed to agree on the layout.

The binary already contained all the knowledge needed to deploy itself — directory paths, service unit contents, configuration defaults, permission requirements. The shell scripts were compensating for that knowledge not being exercised.

## Decision

Collapse all deployment logic into the `garden-moss` binary. One binary handles fresh install, update, OS provisioning, and pre-start staged deployment. Shell scripts are eliminated from the critical path.

### Subcommand surface

```
garden-moss install [-y|--yes] [--dry-run]   # Fresh install or update (interactive)
garden-moss uninstall                         # Remove service and binaries (data preserved)
garden-moss pre-start [--dry-run]             # Process staged packages (ExecStartPre)
garden-moss                                   # Daemon mode (started by service manager)
```

All three subcommands (`install`, `uninstall`, `pre-start`) run synchronously in `main()` before the Tokio runtime — they must never activate the daemon loop.

### Interactive provisioning model

`garden-moss install` detects missing environment components and prompts the user:

```
Environment check:
  Docker          not installed
  stone user      not found
  avahi-daemon    not installed
  DNS resolution  not configured

  Set up missing components? [Y/n]
```

If all components are present, the check shows green and no prompt appears. The `--yes` flag auto-accepts all prompts for non-interactive contexts (USB preseed, `install.sh` one-liner, CI/CD).

On Windows, Docker Desktop is installed via `winget install Docker.DockerDesktop --silent` when available.

### Ephemeral mode nudge

When `garden-moss` runs without `install` and is not registered as a service, it logs a warning:

```
WARN Moss is running in ephemeral mode (not installed as a service)
WARN To install permanently: sudo garden-moss install
```

The daemon still runs normally — this is just a nudge, not a blocker.

### `garden-moss install` — unified install and update

The command auto-detects whether this is a fresh install or an update by inspecting the system:

| Signal | Mode | Behavior |
|--------|------|----------|
| No service registered, no binaries in install dir | **Fresh install** | Full setup, register service, start |
| Service exists, binaries present | **Update** | Stop service, deploy new files, restart |
| Service exists but broken/stopped | **Repair** | Re-deploy files, re-register if needed, start |

**Phase 1 — Resolve package**

Resolution order:
1. Check `{staging_dir}/validated/` (pre-staged by deploy API)
2. Check sibling directory for `zen-garden-*-{platform}.{ext}`
3. Download from GitHub Releases (`GET /repos/{owner}/{repo}/releases/latest`, find matching asset, download with progress, verify SHA256)
4. Graceful failure with download instructions

**Phase 2 — Deploy files**

- Extract package to temp directory
- Copy binaries to install dir (`/usr/local/bin/` on Linux, `C:\ProgramData\ZenGarden\` on Windows)
- Copy companions to companions dir
- Handle external tools (install/uninstall/retire)
- Write version breadcrumb to `{data_dir}/installed-version.json`

**Phase 3 — Generate system configuration**

The binary writes all system files directly — no filesystem-mirror tree, no shipped templates:

| File | Generated by | When |
|------|-------------|------|
| `/etc/systemd/system/garden-moss.service` | `generate_unit_file()` (already exists in `linux.rs`) | Every install/update |
| `/etc/zen-garden/garden-moss.toml` | Default config with port and log level | Fresh install only (never overwrite user config) |
| `/etc/sudoers.d/moss` | Passwordless sudo for `stone` user | `--yes` only |
| `/etc/systemd/resolved.conf.d/zen-garden.conf` | mDNS resolve config | `--yes` only |

On Windows:
| File | Generated by | When |
|------|-------------|------|
| Windows Service registration | `sc create` / stop-delete-wait-recreate | Every install/update |
| Firewall rules | `netsh` via `koi_common::firewall` | Every install/update |

**Phase 4 — Register and start service**

- Linux: `systemctl daemon-reload`, `systemctl enable`, `systemctl start`
- Windows: Service registration with recovery policy, `sc start`
- Both: Poll `/health` endpoint for up to 10 seconds, report status

**Phase 5 — Print summary**

Adapts output based on mode:
- Fresh: "Zen Garden is ready." with API URL, CLI hint, service commands
- Update: "Updated Zen Garden (0.2.100 → 0.2.200)" with version delta

### `garden-moss install --yes` — OS-level setup

Opt-in flag that performs system provisioning. All steps are idempotent (check before acting, skip if already present):

| Step | What | Check |
|------|------|-------|
| Create `stone` user | `useradd -m -s /bin/bash stone` | `id stone` succeeds |
| Set password | `echo stone:stone \| chpasswd` | Skipped if user exists |
| Passwordless sudo | Write `/etc/sudoers.d/moss` | File exists |
| Docker group | `usermod -aG docker stone` | Already in group |
| Install Docker | `apt-get install -y docker.io` | `command -v docker` |
| Enable Docker | `systemctl enable --now docker` | Already enabled |
| Install avahi | `apt-get install -y avahi-daemon` | `command -v avahi-daemon` |
| Enable avahi | `systemctl enable --now avahi-daemon` | Already enabled |
| Configure resolved | Write `zen-garden.conf` to `/etc/systemd/resolved.conf.d/` | File exists |
| Enable resolved | `systemctl enable --now systemd-resolved` | Already enabled |
| Enable networkd | `systemctl enable --now systemd-networkd` | Already enabled |
| Mask wait-online | `systemctl mask systemd-networkd-wait-online` | Already masked |
| Symlink resolv.conf | `ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf` | Already symlinked |
| Set timezone | `timedatectl set-timezone` from config | Already correct |
| Enable NTP | `timedatectl set-ntp true` | Already enabled |
| Set directory ownership | `chown stone:stone` on data/config dirs | Already owned |

On update, `--yes` is safe but unnecessary — the provisioning steps are no-ops when the system is already configured.

Windows has no `--yes` equivalent — Docker Desktop is a GUI install, there is no `stone` user concept, and DNS/mDNS are handled differently.

### `garden-moss pre-start` — staged deployment

Replaces `moss-update-helper.sh` as the systemd `ExecStartPre` command:

```ini
ExecStartPre=/usr/local/bin/garden-moss pre-start
```

Logic:
1. Check `{data_dir}/staging/validated/` for staged packages
2. If present: copy binaries to install dir, apply permissions
3. Clean up staging directory
4. If systemd unit files were updated: `systemctl daemon-reload`
5. Exit 0 (Moss daemon starts normally)

If no staged packages exist, exits immediately (no-op). This runs on every service start, so it must be fast.

The `pre-start` subcommand does NOT:
- Download packages (that's `install`'s job)
- Register or re-register the service (that's `install`'s job)
- Provision the OS (that's `--yes`'s job)
- Start the daemon (that's systemd's job)

### Package simplification

The package format loses the `scripts/` directory. All system configuration is generated by the binary:

**Before** (BUILD-0002):
```
zen-garden-{version}-linux-x64/
├── bin/
│   ├── garden-moss
│   ├── garden-rake
│   ├── garden-lantern
│   ├── moss-update-helper.sh          ← eliminated
│   └── companions/
├── scripts/                            ← eliminated
│   ├── var/lib/zen-garden/
│   │   └── garden.conf                ← eliminated
│   └── etc/systemd/system/
│       └── garden-moss.service        ← eliminated
└── package.json
```

**After**:
```
zen-garden-{version}-linux-x64/
├── bin/
│   ├── garden-moss
│   ├── garden-rake
│   ├── garden-lantern
│   ├── tools/
│   │   └── (external tool binaries)
│   └── companions/
│       ├── cricket/garden-cricket
│       └── firefly/garden-firefly
└── package.json
```

Windows package structure is unchanged (it never had `scripts/`).

### USB installer simplification

The NewStone preseed `late_command` reduces to:

```bash
cp /cdrom/garden-moss /target/tmp/garden-moss
cp /cdrom/zen-garden-*.tar.gz /target/tmp/
chmod +x /target/tmp/garden-moss
in-target /tmp/garden-moss install --yes
rm /target/tmp/garden-moss /target/tmp/zen-garden-*.tar.gz
```

Eliminated from the USB creation process:
- `stone-root/` filesystem mirror (currently just a README.md — already vestigial)
- `package-assets/garden-moss.service` (generated by binary)
- `installer/moss-update-helper.sh` (replaced by `pre-start` subcommand)
- `installer/garden.conf` (timezone/NTP handling moves into `--yes`)
- `installer/sudoers.d-moss` (written by binary during `--yes`)
- Template-based service file generation in `NewStone-linux-x64.ps1`

The USB creator script places two files on the USB (binary + package) and writes the preseed. Everything else is handled by `garden-moss install --yes` running in-target.

### Deploy API flow (network updates)

The `POST /api/v1/stone:deploy` endpoint continues to work as before:

1. Receive package, validate SHA256
2. Extract and stage to `{data_dir}/staging/validated/`
3. If package contains `garden-moss`: trigger service restart
4. On restart, `ExecStartPre` runs `garden-moss pre-start`, which deploys staged files
5. Moss daemon starts with new binaries

The only change is that `ExecStartPre` calls `garden-moss pre-start` instead of `moss-update-helper.sh`.

### Windows update flow

The existing Windows update mechanism (`spawn_windows_updater` → `finalize_service_update`) is complex but functional. This ADR does not change it. The `garden-moss install` command already handles the Windows upgrade path (stop → delete → wait → recreate), and the deploy API's temp-updater flow handles the hot-update case where Moss must replace its own running binary.

Future work could unify these paths, but that carries risk for a working mechanism with extensive logging and error recovery.

### One-liner install scripts

Thin wrapper scripts that download the binary and run it:

**Linux** (`install.sh`):
```bash
#!/bin/bash
set -euo pipefail
REPO="sylin-org/zen-garden"
ARCH=$(uname -m)
case "$ARCH" in x86_64) PLATFORM="linux-x64" ;; i686|i386) PLATFORM="linux-x86" ;; *) echo "Unsupported: $ARCH"; exit 1 ;; esac

echo "Fetching latest release..."
RELEASE=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
VERSION=$(echo "$RELEASE" | grep -o '"tag_name":"[^"]*"' | cut -d'"' -f4)
MOSS_URL=$(echo "$RELEASE" | grep -o "\"browser_download_url\":\"[^\"]*garden-moss-${PLATFORM}[^\"]*\"" | cut -d'"' -f4)
PKG_URL=$(echo "$RELEASE" | grep -o "\"browser_download_url\":\"[^\"]*zen-garden-[^\"]*-${PLATFORM}\.tar\.gz\"" | cut -d'"' -f4)

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading garden-moss ($VERSION)..."
curl -fsSL -o "$TMPDIR/garden-moss" "$MOSS_URL"
chmod +x "$TMPDIR/garden-moss"

echo "Downloading package..."
curl -fsSL -o "$TMPDIR/$(basename "$PKG_URL")" "$PKG_URL"

echo "Installing..."
"$TMPDIR/garden-moss" install "$@"
```

Usage:
```bash
curl -fsSL https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/install.sh | sudo bash
curl -fsSL https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/install.sh | sudo bash -s -- --provision
```

**Windows** (`install.ps1`): equivalent PowerShell script using `Invoke-RestMethod`.

These scripts are ~30 lines each. Their only job is downloading two files and running `garden-moss install`. All intelligence stays in the Rust binary.

### Version breadcrumb

On every successful install/update, Moss writes `{data_dir}/installed-version.json`:

```json
{
  "version": "0.2.202603161200",
  "installed_at": "2026-03-16T12:05:30Z",
  "platform": "linux-x64",
  "method": "install"
}
```

The `method` field records how the install happened (`install`, `deploy-api`, `pre-start`). This enables:
- Version comparison on update: "Updating 0.2.100 → 0.2.200"
- Diagnostics: when was this stone last updated, and how?
- Future: nourishment can compare installed version against GitHub releases

## Consequences

### Positive

- **Single source of truth**: All deployment logic lives in one place (Rust). No Bash/PowerShell/template divergence.
- **Complete fresh install**: `garden-moss install --yes` takes a bare Debian machine to a running stone. No manual steps, no separate scripts.
- **Simpler packages**: The `scripts/` directory and filesystem-mirror convention are eliminated. Packages contain only binaries and a manifest.
- **Simpler USB installer**: `NewStone-linux-x64.ps1` places two files and writes a one-line late_command. No template generation, no stone-root assembly.
- **Simpler build system**: `build-linux-x64.ps1` no longer assembles a `scripts/` tree or converts line endings on shell scripts.
- **Testable**: Rust code is unit-testable. Shell scripts were not tested.
- **One-liner install**: `curl ... | sudo bash` becomes possible because the binary is self-sufficient.
- **Auditable upgrades**: Version breadcrumb provides install history.

### Negative

- **Larger binary scope**: `garden-moss` now contains provisioning logic (apt-get, user creation, DNS config). This is ~200 lines of `std::process::Command` calls — the same complexity as the shell scripts, just in Rust.
- **`pre-start` adds a Moss dependency to boot**: If the `garden-moss` binary is corrupted, `ExecStartPre` fails and the service won't start. With the shell script, a corrupted Moss binary could still have its helper deploy a staged fix. Mitigation: the `pre-start` subcommand is pure file copy with no dependencies beyond the binary itself. If the binary is corrupt, the staged update can't help anyway — the service needs the binary to run.
- **`--yes` is Linux-only**: Windows provisioning (Docker Desktop, etc.) remains manual. This reflects the reality that Windows stones are a secondary platform.

### Neutral

- **Windows update flow unchanged**: The temp-updater mechanism (`spawn_windows_updater` → `finalize_service_update`) continues as-is. Unifying it with `install` is future work.
- **deploy.ps1 unchanged**: The network deployment script continues to POST packages to the deploy API. The only downstream change is `ExecStartPre` calling `garden-moss pre-start` instead of `moss-update-helper.sh`.
- **Package format version**: The `package.json` manifest format is unchanged. Packages without `scripts/` are forward-compatible — older Moss versions simply find no scripts to deploy.

## Implementation

### Phase 1: Fix existing gaps

1. Add `systemctl start` to `linux.rs` (currently only enables, never starts)
2. Add default `garden-moss.toml` creation (fresh install only, skip if exists)
3. Add directory ownership (`chown stone:stone` on data/config dirs)
4. Write version breadcrumb on successful install

### Phase 2: Add `pre-start` subcommand

1. Add `PreStart` variant to `Commands` enum in `cli.rs`
2. Implement staged file deployment (port logic from `moss-update-helper.sh`)
3. Update `generate_unit_file()` to use `ExecStartPre=/usr/local/bin/garden-moss pre-start`
4. Test: stage files manually, run `garden-moss pre-start`, verify deployment

### Phase 3: Add `--yes` flag

1. Add `--yes` flag to `Install` variant in `cli.rs`
2. Implement idempotent provisioning steps (user, Docker, avahi, resolved, permissions)
3. Test on bare Debian VM

### Phase 4: Simplify package and build

1. Remove `scripts/` assembly from `build-linux-x64.ps1`
2. Remove `moss-update-helper.sh` from package assets list in `dist.json`
3. Remove `garden.conf` from package assets
4. Remove `package-assets/garden-moss.service` (generated by binary)
5. Update `dist.json` to remove `shellScripts`, `serviceFile`, `gardenConfig` from assets

### Phase 5: Simplify USB installer

1. Update `NewStone-linux-x64.ps1` to use `garden-moss install --yes` in preseed late_command
2. Remove template-based service file generation
3. Remove `stone-root/` directory population logic
4. Test USB install on bare hardware

### Phase 6: One-liner install scripts and GitHub Releases

1. Implement GitHub release download in `package.rs` (replace TODO at line 69)
2. Write `installer/install.sh` (Linux one-liner wrapper)
3. Write `installer/install.ps1` (Windows one-liner wrapper)
4. Set up GitHub Actions to build and publish releases with platform assets

### Verification

```bash
# Fresh install on bare Debian
sudo garden-moss install --yes
curl http://localhost:7185/health
systemctl status garden-moss
cat /var/lib/zen-garden/installed-version.json

# Update with local package
sudo garden-moss install    # detects existing install, shows version delta

# Pre-start staged deployment
sudo cp -r /tmp/test-staging/* /var/lib/zen-garden/staging/validated/
sudo garden-moss pre-start
ls -la /usr/local/bin/garden-moss

# Network deploy (unchanged)
./deploy.ps1
# Verify ExecStartPre calls garden-moss pre-start (not moss-update-helper.sh)
systemctl cat garden-moss | grep ExecStartPre
```

## Artifacts retired

| Artifact | Replacement | When |
|----------|------------|------|
| `installer/setup-stone.sh` | `garden-moss install --yes` | Phase 3 |
| `installer/moss-update-helper.sh` | `garden-moss pre-start` | Phase 2 |
| `installer/garden-upgrade.sh` | Already deprecated | Phase 4 |
| `installer/garden.conf` | Timezone/NTP in `--yes` | Phase 3 |
| `installer/package-assets/garden-moss.service` | `generate_unit_file()` in `linux.rs` | Phase 4 |
| `installer/sudoers.d-moss` | Written by `--yes` | Phase 3 |
| `installer/stone-root/` | Two-file preseed | Phase 5 |
| `scripts/` directory in packages | Not generated | Phase 4 |

## References

- [BUILD-0002: Unified Deployment Packages](BUILD-0002-unified-deployment-packages.md) — package format and staging conventions
- [Moss Self-Install proposal](../proposals/moss-self-install.md) — original design for `garden-moss install`
- [Installing Moss guide](../guides/installing-moss.md) — operator documentation (will need update)
- Current implementation: `src/moss/src/infra/installer/` (`mod.rs`, `linux.rs`, `windows.rs`, `package.rs`)
- Current shell scripts: `installer/setup-stone.sh`, `installer/moss-update-helper.sh`
- Current build config: `installer/dist.json`, `installer/build-linux-x64.ps1`
- Koi reference implementation: `koi/src/platform/unix.rs`, `koi/src/platform/windows.rs`
