# Nurturing System TODO List

Tracked gaps and enhancements for the nurturing/backup system.
Priority: P0 (critical), P1 (high), P2 (medium), P3 (nice-to-have)

---

## CLI Commands (Rake)

### P1: Add restore commands
- [ ] `garden-rake restore {offering} from slot A|B`
  - Stop offering, restore from local A/B slot, restart
  - Default to most recent slot if not specified
  - Show restore progress and result
- [ ] `garden-rake restore {offering} from seed-bank {name}`
  - Stop offering, restore from remote seed bank, restart
  - Option to specify harvest_id or default to latest
  - Show size and timestamp before confirming
- [ ] `garden-rake restore {offering} --dry-run`
  - Preview what would be restored without executing
  - Show source, size, timestamp, affected volumes

### P2: Add timer/schedule commands
- [ ] `garden-rake schedule {offering} every {interval}`
  - Create systemd timer (Linux) or Task Scheduler task (Windows)
  - Support intervals: `1h`, `6h`, `12h`, `24h`, `weekly`
  - Optionally add random delay for staggering
- [ ] `garden-rake schedule list`
  - Show all configured nurturing schedules
  - Include next run time and last run status
- [ ] `garden-rake schedule remove {offering}`
  - Remove scheduled nurturing for an offering
  - Clean up timer/task files

### P2: Add status commands
- [ ] `garden-rake status nurturing`
  - Show all offerings with backup status
  - Last local backup time and slot
  - Last replication time and target
  - Storage used by backups
- [ ] `garden-rake status nurturing {offering}`
  - Detailed view for single offering
  - Both local slots with timestamps
  - All remote copies across seed banks
  - Retention policy status

### P3: Add backup management commands
- [ ] `garden-rake nurturing trigger {offering}`
  - CLI wrapper for trigger endpoint
  - Show workflow progress and result
- [ ] `garden-rake nurturing trigger-all`
  - Trigger all offerings on current stone
- [ ] `garden-rake nurturing prune {offering}`
  - Manually prune old backups beyond retention
  - Support `--local` and `--remote` flags
- [ ] `garden-rake nurturing list {offering}`
  - List all backups (local and remote) for offering
  - Include size, timestamp, location

---

## Policy Configuration

### P2: Make retention configurable
- [ ] Add retention settings to `garden-moss.toml`
  ```toml
  [nurturing]
  local_retention = 2          # A/B slots (fixed at 2)
  remote_retention = 5         # per offering per seed bank
  prune_on_startup = true      # clean up excess on boot
  ```
- [ ] API endpoint to read/update retention
  - `GET /api/v1/stone/nurturing/config`
  - `PATCH /api/v1/stone/nurturing/config`
- [ ] Per-offering retention override
  - Allow critical offerings to have more copies

### P2: Make routing strategy configurable
- [ ] Add routing to config file
  ```toml
  [nurturing.routing]
  strategy = "first"           # first, most_capacity, all
  max_attempts = 3             # failover retry count
  ```
- [ ] API endpoint to change routing
- [ ] Per-offering routing override

### P3: Add backup scheduling to config
- [ ] Define schedules in config (alternative to systemd)
  ```toml
  [nurturing.schedule]
  default_interval = "24h"
  random_delay = "1h"

  [nurturing.schedule.overrides]
  mongodb = "6h"               # more frequent for critical
  ```

---

## Recovery Features

### P1: Cross-stone recovery
- [ ] Document recovery procedure for stone failure
- [ ] `garden-rake recover {offering} from seed-bank {name} --to {stone}`
  - Allow installing and restoring on different stone
  - Handle offering_id mapping
- [ ] Seed bank manifest includes full offering metadata
  - Manifest version, volumes, ports, environment
  - Enables recreation without original stone

### P2: Partial restore
- [ ] Allow restoring specific volumes only
  - `garden-rake restore {offering} --volumes data,config`
- [ ] Allow restoring to different path (for inspection)
  - `garden-rake restore {offering} --to /tmp/inspect`

### P3: Backup verification
- [ ] Checksum verification after replication
- [ ] Periodic integrity check of remote backups
- [ ] `garden-rake nurturing verify {offering}`

---

## Storage Visibility

### P1: Backup storage metrics
- [ ] Track storage used by local backups per offering
- [ ] Track storage used on each seed bank
- [ ] Include in `/api/v1/stone/nurturing` response
- [ ] Add to stone dashboard (see UI section)

### P2: Seed bank health monitoring
- [ ] Check seed bank connectivity periodically
- [ ] Alert when seed bank goes offline
- [ ] Alert when seed bank is nearly full
- [ ] Include health status in API response

### P3: Storage analytics
- [ ] Backup size trends over time
- [ ] Deduplication opportunities (if using btrfs)
- [ ] Capacity planning recommendations

---

## UI Enhancements (Stone Webpage)

### P1: Add nurturing section to dashboard
- [ ] Per-offering backup status card
  - Last backup time (local and remote)
  - Next scheduled backup
  - Storage used
  - Quick actions: backup now, restore
- [ ] Seed bank status panel
  - Connected seed banks
  - Capacity/usage per bank
  - Online/offline status

### P2: Backup history view
- [ ] Timeline of backups per offering
- [ ] Filter by local/remote
- [ ] Show success/failure
- [ ] One-click restore from any point

### P3: Storage breakdown visualization
- [ ] Pie chart of storage by category
  - Offering data
  - Local backups
  - Containers/images
  - System
- [ ] Per-offering storage detail

---

## Notifications & Alerts

### P2: Backup notifications
- [ ] Event on successful backup
- [ ] Event on backup failure
- [ ] Event on retention prune
- [ ] Integrate with SSE for real-time UI updates

### P3: Alert channels
- [ ] Webhook for backup events
- [ ] Email notifications (optional)
- [ ] Structured logging for monitoring systems

---

## Security

### P2: Backup encryption
- [ ] Encrypt backups at rest on seed bank
- [ ] Key management (per-stone or per-pool)
- [ ] Decrypt on restore

### P3: Access control
- [ ] Restrict restore operations to admin
- [ ] Audit log for backup/restore operations

---

## Testing

### P1: Expand probe tests
- [ ] Test restore from local slot
- [ ] Test restore from seed bank
- [ ] Test retention pruning behavior
- [ ] Test failover when primary seed bank unavailable

### P2: Chaos testing
- [ ] Test backup during offering update
- [ ] Test backup with disk full
- [ ] Test restore with corrupted archive

---

## Documentation

### P1: Disaster recovery guide
- [ ] Step-by-step recovery from total stone loss
- [ ] Recovery from partial data loss
- [ ] Best practices for backup strategy

### P2: Operations runbook
- [ ] Common backup issues and solutions
- [ ] Monitoring recommendations
- [ ] Capacity planning guide

---

## Implementation Notes

### Dependencies between items
- Cross-stone recovery requires seed bank manifest enhancement
- UI enhancements require storage metrics API
- Encryption requires key management design first

### Suggested implementation order
1. Rake restore commands (unblocks manual recovery)
2. Storage metrics API (needed for UI)
3. Dashboard nurturing section (user visibility)
4. Schedule commands (automate setup)
5. Cross-stone recovery (disaster preparedness)
6. Encryption (security hardening)

---

*Last updated: 2026-01-30*
