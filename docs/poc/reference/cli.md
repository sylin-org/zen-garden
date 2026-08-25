# Garden-Rake CLI Reference

Complete command reference for `garden-rake`, the Zen Garden management CLI.

All commands follow a single grammar: `verb [noun] [--flags]`.

---

## Quick Start

```bash
garden-rake                          # Show command directory
garden-rake offer                    # Browse available offerings
garden-rake offer mongodb            # Install MongoDB
garden-rake list                     # List services on stone
garden-rake observe                  # View entire garden state
```

---

## Global Options

Every command accepts these flags:

| Flag | Short | Description |
|------|-------|-------------|
| `--quiet` | `-q` | Suppress suggestions and hints |
| `--fresh` | | Clear cached tending state and force fresh discovery |
| `--verbose` | `-v` | Increase verbosity (`-v` info, `-vv` debug, `-vvv` trace) |
| `--output <FORMAT>` | `-o` | Output format: `human` (default) or `json` |
| `--field <PATH>` | | Extract a specific field using dot notation (e.g., `services[0].connection.uris[0]`) |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ZG_STONE` | Skip discovery and target this stone endpoint directly |
| `ZG_NO_COLOR` | Disable colored output |
| `ZG_UNICODE` | Force unicode symbols on/off |
| `ZG_QUIET` | Same as `--quiet` |

---

## Stone Targeting

Most commands target a specific stone. Resolution order:

1. **`--at <endpoint>`** flag (explicit URL or stone name)
2. **`ZG_STONE`** environment variable
3. **Tending state** (cached via `garden-rake tend`)
4. **Auto-discovery** via UDP multicast/broadcast

Use `--at` (or its alias `--on`) to target a specific stone:

```bash
garden-rake list --at stone-crystal-forest
garden-rake list --at http://192.168.1.100:7185
```

---

## Discovery Commands

### `observe`

View a snapshot of the entire garden (all stones and their services).

```bash
garden-rake observe                              # All stones
garden-rake observe stone-01                     # Specific stone
garden-rake observe --offering mongodb,redis     # Filter by offerings
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | No | Filter to a specific stone |
| `--offering <NAMES>` | No | Comma-separated offering filter |

### `status`

Get detailed status of the tended (or targeted) stone.

```bash
garden-rake status
garden-rake status --at stone-01
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--at <ENDPOINT>` | No | Target stone |

### `list`

List all services running on a stone.

```bash
garden-rake list
garden-rake list --at stone-01
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--at <ENDPOINT>` | No | Target stone |

### `watch`

Stream real-time events from a stone. Press Ctrl+C to stop.

```bash
garden-rake watch                                    # Watch tended stone events
garden-rake watch --until 'completed'                # Exit when string appears
garden-rake watch offering mongodb logs              # Stream offering container logs
garden-rake watch offering mongodb logs --timestamps # With timestamps
garden-rake watch stone stone-01 logs                # Stream all logs from a stone
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--at <ENDPOINT>` | No | Target stone |
| `--until <STRING>` | No | Exit when this string appears in the stream |

**Subcommands:**

| Subcommand | Description |
|------------|-------------|
| `offering <name> logs [--timestamps]` | Stream logs for a specific offering |
| `stone <name> logs [--timestamps]` | Stream all logs from a stone |

### `find`

Find running services across the garden by name, category, or tag.

```bash
garden-rake find mongodb                     # By name
garden-rake find c:database                  # By category
garden-rake find t:nosql                     # By tag
garden-rake find mongodb --format uri        # Just the connection string
garden-rake find mongodb --format uri-ip     # Connection string with IP fallback
garden-rake find mongodb --ensure            # Auto-provision if not found
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<query>` | Yes | Search query (name, `c:category`, or `t:tag`) |
| `--format <FMT>` | No | Output: `human` (default), `json`, `uri`, `uri-ip` |
| `--ensure` | No | Auto-provision the service if not found |
| `--at <ENDPOINT>` | No | Target stone |

### `config`

Get detailed service configuration (connection URIs, ports, protocol).

```bash
garden-rake config mongodb                              # Full config
garden-rake config mongodb --output json                # JSON output
garden-rake config mongodb --field connection.uri       # Just the URI
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to query |
| `--at <ENDPOINT>` | No | Target stone |

### `logs`

Shortcut for streaming offering container logs (equivalent to `watch offering <name> logs`).

```bash
garden-rake logs mongodb                     # Stream logs for mongodb
garden-rake logs mongodb --timestamps        # With timestamps
garden-rake logs mongodb --at stone-01       # From a specific stone
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to stream logs for |
| `--timestamps` | No | Include timestamps in log output |
| `--at <ENDPOINT>` | No | Target stone |

---

## Service Lifecycle Commands

### `offer`

Browse the offering catalog or install a service.

```bash
garden-rake offer                                # List all offerings by category
garden-rake offer mongodb                        # Install mongodb
garden-rake offer mongodb info                   # Show details + compatibility
garden-rake offer mongodb --prefer ssd,nvme      # Bias for hardware preferences
garden-rake offer mongodb --anywhere-on-fail     # Try all stones if local fails
garden-rake offer mongodb --placement-mode auto  # Auto-place without prompting
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<offering>` | No | Offering name (omit to list all) |
| `info` | No | Show offering details and compatibility |
| `--prefer <PREFS>` | No | Comma-separated hardware preferences (e.g., `ssd`, `nvme`) |
| `--anywhere-on-fail` | No | If local install fails, try other stones |
| `--placement-mode <MODE>` | No | `interactive` (default) or `auto` |
| `--at <ENDPOINT>` | No | Target stone |

**Alias:** `explore`

### `rest`

Stop a running service (put it in rest mode).

```bash
garden-rake rest mongodb
garden-rake rest mongodb --at stone-01
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to stop |
| `--at <ENDPOINT>` | No | Target stone |

### `wake`

Start a service that is in rest mode.

```bash
garden-rake wake mongodb
garden-rake wake mongodb --at stone-01
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to start |
| `--at <ENDPOINT>` | No | Target stone |

### `upgrade`

Upgrade a service (or all services) to the latest image.

```bash
garden-rake upgrade mongodb                  # Upgrade one service
garden-rake upgrade --all                    # Upgrade all services on stone
garden-rake upgrade --garden                 # Cross-stone upgrade for all stones
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | No | Service name (omit with `--all`) |
| `--all` | No | Upgrade all services on stone |
| `--garden` | No | Garden-wide upgrade across all stones |
| `--at <ENDPOINT>` | No | Target stone |

### `nourish`

Check and apply updates across the garden — offerings, Moss, and firmware/BIOS.

```bash
garden-rake nourish                          # Interactive: check all, choose what to apply
garden-rake nourish --updates-only           # Check without applying
garden-rake nourish --yes                    # Auto-confirm all updates
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | No | Target specific stone (omit for garden-wide) |
| `--updates-only` | No | Check for updates without applying |
| `--yes` / `-y` | No | Auto-confirm all updates |

Interactive menu options: **[A]** All updates, **[O]** Offering updates only, **[F]** Firmware updates only.

### `remove`

Soft-delete a service. The container is preserved as a stray (can be re-adopted).

```bash
garden-rake remove mongodb
garden-rake remove mongodb --yes             # Skip confirmation
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to remove |
| `--yes` | No | Skip confirmation prompt |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

### `uproot`

Hard-delete a service. The container is destroyed permanently and cannot be recovered.

```bash
garden-rake uproot mongodb
garden-rake uproot mongodb --yes             # Skip confirmation
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to destroy |
| `--yes` | No | Skip confirmation prompt |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

---

## Adoption & Borrowing Commands

### `adopt`

Adopt an existing container into Zen Garden management.

```bash
garden-rake list --strays                    # List adoptable containers first
garden-rake adopt my-mongodb-container       # Adopt a container
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<container>` | Yes | Container name to adopt |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

### `release`

Release an adopted service. Stops managing it but keeps the container running.

```bash
garden-rake release mongodb
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<service>` | Yes | Service name to release |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

### `borrow`

Register an external network service for discovery (not managed, just referenced).

```bash
garden-rake borrow redis --from redis://company-cache:6379
garden-rake borrow postgres --from postgresql://db-server:5432
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<name>` | Yes | Name for this borrowed service |
| `--from <URL>` | No | URL/connection string for the external service |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

### `return`

Unregister a borrowed service (does not affect the external service itself).

```bash
garden-rake return redis
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<name>` | Yes | Name of the borrowed service to unregister |
| `--at <ENDPOINT>` | No | Target stone (alias: `--on`) |

### Listing filters

The `list` command supports filters for adoption and borrowing status:

```bash
garden-rake list --adopted                   # List adopted services only
garden-rake list --borrowed                  # List borrowed services only
garden-rake list --strays                    # List containers available for adoption
```

---

## Capabilities Commands

### `capabilities`

Manage sub-capabilities for offerings that support them (e.g., Ollama models).

```bash
garden-rake capabilities ollama                            # List models
garden-rake capabilities add ollama llama3                 # Pull a model
garden-rake capabilities add ollama phi --dry-run          # Validate only
garden-rake capabilities remove ollama phi                 # Remove a model
garden-rake capabilities refresh ollama                    # Update all models
garden-rake capabilities refresh ollama --type model       # Update specific type
garden-rake capabilities ollama mirror from stone-01       # Mirror from another stone
garden-rake capabilities ollama mirror to stone-02         # Mirror to another stone
garden-rake capabilities ollama mirror from stone-01 to stone-02
```

**Alias:** `cap`

**Subcommands:**

| Subcommand | Arguments | Description |
|------------|-----------|-------------|
| *(none)* | `<offering>` | List capabilities for an offering |
| `add` | `<offering> <name> [--type TYPE] [--dry-run]` | Add a capability |
| `remove` | `<offering> <name> [--type TYPE]` | Remove a capability |
| `refresh` | `<offering> [--type TYPE] [--dry-run]` | Update all capabilities |
| `mirror` | `<offering> from <stone> [to <stone>]` | Mirror capabilities between stones |

---

## Stone Administration Commands

### `stone wake`

Wake a sleeping stone via Wake-on-LAN magic packet.

```bash
garden-rake stone wake oak                       # Wake stone 'oak'
garden-rake stone wake oak --at cedar            # Send WoL from 'cedar'
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | Yes | Stone name to wake |
| `--at <ENDPOINT>` | No | Stone to send WoL packet from |

### `stone shutdown`

Power off (shut down) a stone.

```bash
garden-rake stone shutdown                       # Shut down tended stone
garden-rake stone shutdown oak                   # Shut down 'oak' by name
garden-rake stone shutdown --at oak              # Same as above
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | No | Stone name, ID, or URL (omit for tended stone) |
| `--at <ENDPOINT>` | No | Alternative to positional argument |

### `stone reboot`

Reboot a stone.

```bash
garden-rake stone reboot                         # Reboot tended stone
garden-rake stone reboot oak                     # Reboot 'oak' by name
garden-rake stone reboot --at oak                # Same as above
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | No | Stone name, ID, or URL (omit for tended stone) |
| `--at <ENDPOINT>` | No | Alternative to positional argument |

### `stone install`

Install Moss as a system service. Requires administrator/root privileges.

```bash
garden-rake stone install                        # Install on local stone
garden-rake stone install --at stone-01          # Install on specific stone
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--at <ENDPOINT>` | No | Target stone |

### `stone verbosity`

Control console output verbosity on a stone.

```bash
garden-rake stone verbosity sing                 # Verbose output (30-min timeout)
garden-rake stone verbosity sing --forever       # Verbose output permanently
garden-rake stone verbosity quiet                # Reset to default (informative)
garden-rake stone verbosity silent               # No console output
garden-rake stone verbosity minimal              # Critical events only
```

**Verbosity modes:**

| Mode | Description |
|------|-------------|
| `sing` | Full debug output (verbose). Times out after 30 minutes unless `--forever` is used. |
| `quiet` | Major lifecycle events (default/informative) |
| `minimal` | Critical events only |
| `silent` | No console output (for headless/service use) |

### `stone reconcile`

Force Moss to reconcile its service registry with existing Docker containers.

```bash
garden-rake stone reconcile                      # Adopt any missing containers
garden-rake stone reconcile --drop-invalid       # Also remove invalid containers
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--drop-invalid` | No | Remove `zen-offering-*` containers that don't map to a known template |
| `--at <ENDPOINT>` | No | Target stone |

### `stone refresh`

Update the `garden-moss` or `garden-rake` binary on a remote stone (development use).

```bash
garden-rake stone refresh moss --from ./target/release/garden-moss
garden-rake stone refresh rake --from ./dist/linux-x64/garden-rake
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<component>` | Yes | `moss` or `rake` |
| `--from <PATH>` | Yes | Path to the binary file |
| `--at <ENDPOINT>` | No | Target stone |

---

## Management Commands

### `tend`

Manage which stone rake targets by default. Tending state is cached for 90 seconds.

```bash
garden-rake tend                                 # Show current tending state
garden-rake tend this                            # Tend to localhost
garden-rake tend local                           # Tend to localhost
garden-rake tend auto                            # Auto-discover and set
garden-rake tend http://192.168.1.108:7185       # Set explicit endpoint
garden-rake tend --clear                         # Stop tending
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<target>` | No | `this`, `local`, `auto`, or endpoint URL |
| `--clear` | No | Clear tending state |

---

## Pond Security Commands

Pond commands manage multi-stone trust relationships via koi-certmesh.

### `pond init`

Initialize the pond by creating a CA and placing the keystone.

```bash
garden-rake pond init --passphrase "my secret"
garden-rake pond init --passphrase "my secret" --profile my-team
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--passphrase <PASS>` | Yes | Encrypts the CA private key |
| `--profile <PROFILE>` | No | Trust profile: `just-me` (default), `my-team`, `my-organization` |

### `pond status`

Show pond status and membership.

```bash
garden-rake pond status
```

### `pond invite`

Open enrollment window and generate a TOTP URI for stone admission.

```bash
garden-rake pond invite --passphrase "my secret"
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--passphrase <PASS>` | Yes | CA passphrase to authorize the operation |

### `pond join`

Join an existing pond with a TOTP code.

```bash
garden-rake pond join ABC123
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<code>` | Yes | TOTP code from the keystone's invite |

### `pond unlock`

Unlock the CA private key after a Moss restart.

```bash
garden-rake pond unlock --passphrase "my secret"
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--passphrase <PASS>` | Yes | CA passphrase |

### `pond rename`

Change the pond's display name.

```bash
garden-rake pond rename pond-glacial-heron       # Set specific name
garden-rake pond rename                          # Auto-generate new name
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<name>` | No | New name (`pond-{adj}-{noun}` format). Omit to auto-generate. |

Pond names are decorative identifiers -- renaming has no effect on certificates or security. Names follow a water theme (e.g. `pond-moonlit-basin`, `pond-glacial-heron`, `pond-shallow-lotus`).

### `pond promote`

Promote this stone to standby CA (receive CA key material).

```bash
garden-rake pond promote --passphrase "my secret"
```

| Argument | Required | Description |
|----------|----------|-------------|
| `--passphrase <PASS>` | Yes | CA passphrase |

### `pond drain`

Destroy the pond and revoke all certificates.

```bash
garden-rake pond drain
```

### `pond untrust`

Revoke a stone's certificate and remove it from the pond.

```bash
garden-rake pond untrust stone-02
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<stone>` | Yes | Stone name to revoke |

---

## Companion Commands

Companions extend Moss with additional capabilities like audio feedback (Cricket) and LED displays (Firefly). The `hey` command forwards raw arguments to companions - rake is a thin pass-through.

### `hey tell`

```bash
garden-rake hey tell                             # List all companions
garden-rake hey tell cricket?                    # Show cricket's commands
garden-rake hey tell cricket select mr-robot     # Send command to cricket
garden-rake hey tell cricket volume 50           # Set cricket volume
garden-rake hey tell cricket list                # List cricket's tunes
garden-rake hey tell cricket status              # Show cricket status
garden-rake hey stone-01 tell cricket volume 50  # Send to specific stone
```

### Companion lifecycle

```bash
garden-rake hey tell cricket up                  # Start (enable) companion
garden-rake hey tell cricket down                # Stop (disable) companion
```

### Help system

Append `?` to any token for context-sensitive help:

```bash
garden-rake hey?                                 # Help for 'hey'
garden-rake hey tell?                            # Help for 'tell'
garden-rake hey tell cricket?                    # Show cricket's available commands
```

### Command reference (companion-specific)

Each companion defines its own commands. Use `hey tell <companion>?` to see available commands. Example commands for Cricket (audio companion):

| Command | Arguments | Description |
|---------|-----------|-------------|
| `select` | `<tune>` | Switch to a different tune |
| `list` | | List installed tunes |
| `volume` | `<level>` | Set master volume (0-100) |
| `pull` | `<url>` | Download and install a tune from URL |
| `remove` | `<tune>` | Uninstall a community tune |
| `status` | | Show current tune and settings |

---

## Storage Commands

### `prepare seed-bank`

Prepare a USB device as a seed bank for portable backup storage. **This erases all data on the device.**

```bash
garden-rake prepare seed-bank                              # Auto-detect USB
garden-rake prepare seed-bank /dev/sdb                     # Specific device
garden-rake prepare seed-bank --name garden-data           # Custom name
garden-rake prepare seed-bank --random                     # Random whimsical name
garden-rake prepare seed-bank --fs ext4                    # Use ext4 (default: btrfs)
garden-rake prepare seed-bank --group primary --replica 1  # Replicated bank
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<device>` | No | Device path (auto-detect if omitted) |
| `--name <NAME>` | No | Seed bank name |
| `--random` | No | Generate random whimsical name |
| `--fs <TYPE>` | No | Filesystem: `btrfs` (default) or `ext4` |
| `--group <GROUP>` | No | Logical group for replicated banks |
| `--replica <N>` | No | Replica number within group |
| `--at <ENDPOINT>` | No | Target stone |

### `seed-banks`

List all seed banks and eligible devices on a stone.

```bash
garden-rake seed-banks
garden-rake seed-banks --at stone-01
```

### `release-seed-bank`

Safely unmount a seed bank so the USB device can be physically removed.

```bash
garden-rake release-seed-bank garden-data        # Release specific bank
garden-rake release-seed-bank all                # Release all banks
```

### `store`

S3-compatible object storage operations on seed banks.

```bash
garden-rake store put mydata config.json ./config.json     # Upload file
garden-rake store get mydata config.json ./config.json     # Download file
garden-rake store get mydata config.json                   # Print to stdout
garden-rake store ls mydata                                # List bucket
garden-rake store ls mydata --prefix logs/                 # List with prefix
garden-rake store rm mydata config.json                    # Delete object
garden-rake store head mydata config.json                  # Show metadata
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<operation>` | Yes | `put`, `get`, `ls`, `rm`, `head` |
| `<bucket>` | Yes | Bucket name |
| `<key>` | Depends | Object key (required for put/get/rm/head) |
| `<file>` | No | Local file path (source for put, destination for get) |
| `--prefix <PREFIX>` | No | Prefix filter for list operations |
| `--delimiter <DELIM>` | No | Delimiter for list (default: `/`) |
| `--app <APP>` | No | Application namespace (default: `zen-garden`) |
| `--at <ENDPOINT>` | No | Target stone |

---

## Backup Commands

### `backup status`

Show backup status for offerings.

```bash
garden-rake backup status                        # All offerings
garden-rake backup status mongodb                # Specific offering
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<offering>` | No | Offering name (omit for all) |

### `backup list`

List backups for an offering.

```bash
garden-rake backup list mongodb                  # All backups
garden-rake backup list mongodb --local          # Local backups only
garden-rake backup list mongodb --remote         # Remote backups only
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<offering>` | Yes | Offering name |
| `--local` | No | Local backups only |
| `--remote` | No | Remote backups only |

### `backup trigger`

Trigger a backup for one offering.

```bash
garden-rake backup trigger mongodb
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<offering>` | Yes | Offering name to back up |

### `backup trigger-all`

Trigger backups for all offerings.

```bash
garden-rake backup trigger-all
```

### `backup restore`

Restore an offering from a backup.

```bash
garden-rake backup restore mongodb                              # From current slot
garden-rake backup restore mongodb from slot A                  # From slot A
garden-rake backup restore mongodb from slot B                  # From slot B
garden-rake backup restore mongodb from seed-bank garden-data   # From seed bank
garden-rake backup restore mongodb --dry-run                    # Preview only
garden-rake backup restore mongodb --harvest-id abc123          # Specific harvest
```

| Argument | Required | Description |
|----------|----------|-------------|
| `<offering>` | Yes | Offering name to restore |
| `from slot A\|B` | No | Restore from specific local slot |
| `from seed-bank <name>` | No | Restore from seed bank |
| `--dry-run` | No | Preview without restoring |
| `--harvest-id <ID>` | No | Specific harvest ID (for seed bank) |
| `--at <ENDPOINT>` | No | Target stone |

---

## Reference & Help Commands

### `commands`

Browse the built-in command directory with descriptions and examples.

```bash
garden-rake commands                             # Show all commands by category
garden-rake commands stone install               # Detailed info for a command
garden-rake commands --category system           # Filter by category
```

### `api`

Display the live Moss HTTP API reference (fetched from the running stone).

```bash
garden-rake api                                  # Show all endpoints
garden-rake api --category offerings             # Filter by category
garden-rake api /api/v1/stone/services           # Detailed docs for endpoint
garden-rake api --examples                       # Include curl examples
```

### `template`

Manage offering templates.

```bash
garden-rake template list                        # List available templates
garden-rake template show mongodb                # Show template YAML
```

### `launch`

Open the stone's web portrait in the default browser.

```bash
garden-rake launch                               # Open tended stone's portrait
garden-rake launch --at stone-01                 # Open specific stone
```

---

## Diagnostic Commands

### `election`

Test the distributed election protocol.

```bash
garden-rake election start --election-type update_source
garden-rake election start --election-type ceremony_coordinator
garden-rake election start --criteria '{"moss_version": {"$gt": "0.1.0"}}' --timeout 15
```

### `ceremony`

Run guided workflows (scaffolded, not yet implemented).

```bash
garden-rake ceremony bootstrap                   # First-time setup wizard
garden-rake ceremony migrate                     # Service migration workflow
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error (command failed, API error, etc.) |

---

## See Also

- [Rake Automation Guide](../guides/rake-automation.md) - Scripting and CI/CD integration
- [Connection Strings Reference](connection-strings.md) - Service connection formats
- [Offerings Catalog](offerings.md) - Available service offerings
- [API Reference](api.md) - Moss HTTP API endpoints
