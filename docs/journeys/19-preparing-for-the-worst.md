# Preparing for the Worst

*You hope you never need this. You prepare anyway.*

---

## The Story

Your garden has been running for a year. Three Stones. A dozen services. Everything works. But you've never tested recovery. You've never simulated disaster.

Today, you prepare.

---

### Taking Inventory

First, understand what you have:

```bash
garden-rake observe --verbose
```

```
Garden Inventory

STONES (3):
  stone-amber-ridge    192.168.1.42    Up 387d    8 GB RAM, 256 GB SSD
  stone-coral-reef     192.168.1.58    Up 234d    4 GB RAM, 120 GB HDD
  stone-bronze-canyon  192.168.1.73    Up 89d     32 GB RAM, 500 GB NVMe

OFFERINGS (9):
  stone-amber-ridge:
    ├─ mongodb      7.0.8    4.2 GB data    Last backup: 6h ago
    ├─ redis        7.2.7    512 MB data    Last backup: 6h ago
    └─ grafana      10.3.1   180 MB data    Last backup: 6h ago

  stone-coral-reef:
    ├─ postgres     16.3     8.1 GB data    Last backup: 6h ago
    └─ nginx        1.25.4   50 MB config   Last backup: 6h ago

  stone-bronze-canyon:
    ├─ elasticsearch  8.12.0   12 GB data   Last backup: 6h ago
    ├─ mongodb        8.0.0    2.3 GB data  Last backup: 6h ago
    ├─ ollama         0.1.27   45 GB models Last backup: Never
    └─ minio          latest   28 GB data   Last backup: 6h ago

SEED BANKS (2):
  seed-amber-brook    64 GB USB    On stone-amber-ridge    32 GB used
  seed-coral-cove     128 GB USB   On stone-coral-reef     67 GB used

WARNINGS:
  ⚠ ollama has never been backed up (45 GB data)
  ⚠ minio uses 'latest' tag (version not pinned)
```

You have two seed banks. Most services are backed up every 6 hours. But Ollama—with 45 GB of AI models—has never been backed up. And Minio uses `latest`, which means you don't know exactly what version you're running.

---

### Fixing the Gaps

First, pin the Minio version:

```bash
garden-rake status minio on stone-bronze-canyon
```

```
minio on stone-bronze-canyon

  Image: minio/minio:latest
  Actual digest: sha256:abc123...

  Warning: Using 'latest' tag. Version may change unexpectedly.
  Recommendation: Pin to specific version with 'garden-rake pin minio'.
```

```bash
garden-rake pin minio to RELEASE.2024-01-28T22-35-53Z
```

```
Pinning minio to RELEASE.2024-01-28T22-35-53Z...

  Current: minio/minio:latest (sha256:abc123)
  Pinned:  minio/minio:RELEASE.2024-01-28T22-35-53Z

✓ minio pinned. Future 'nourish' will update from this version.
```

Now you know exactly what version you're running.

Next, back up Ollama. It's large, but it needs protection:

```bash
garden-rake nurturing trigger ollama
```

```
Triggering backup for ollama...

  Creating local snapshot...
    Archiving /root/.ollama (45 GB)... done (12 minutes)

  Replicating to seed banks...
    → seed-amber-brook: insufficient space (32 GB free, need 45 GB)
    → seed-coral-cove: done (67 GB free)

✓ ollama backed up to seed-coral-cove

Note: Consider adding a larger seed bank for full redundancy.
```

Ollama is now backed up. But only to one seed bank—the other doesn't have enough space. You make a note to add a larger USB drive.

---

### Testing Recovery

You decide to test recovery. Not on production—you create a test scenario.

First, check what you'd recover from:

```bash
garden-rake nurturing list mongodb
```

```
Backups for mongodb (stone-amber-ridge):

  LOCAL SLOTS:
    Slot A: 2026-02-15T02:00:00Z  4.2 GB  ← current
    Slot B: 2026-02-14T02:00:00Z  4.1 GB

  SEED BANKS:
    seed-amber-brook:
      └─ 2026-02-15T02:00:00Z  4.2 GB
      └─ 2026-02-14T02:00:00Z  4.1 GB
      └─ 2026-02-13T02:00:00Z  4.1 GB
      └─ 2026-02-12T02:00:00Z  4.0 GB
      └─ 2026-02-11T02:00:00Z  4.0 GB

    seed-coral-cove:
      └─ 2026-02-15T02:00:00Z  4.2 GB
      └─ 2026-02-14T02:00:00Z  4.1 GB
      └─ 2026-02-13T02:00:00Z  4.1 GB
```

Multiple backups across multiple locations. If the Stone dies, you can recover from a seed bank. If a seed bank fails, you have another.

Now, simulate recovery. You stop MongoDB and restore from a seed bank (not local slots):

```bash
# Stop the service
garden-rake rest mongodb on stone-amber-ridge

# Restore from seed bank (not local)
garden-rake restore mongodb from seed-bank seed-coral-cove
```

```
Restoring mongodb from seed-coral-cove...

  Finding latest snapshot... 2026-02-15T02:00:00Z
  Downloading archive (4.2 GB)... done (2 minutes)
  Verifying checksum... passed
  Extracting to volumes... done
  Starting container... done
  Verifying health... passed

✓ mongodb restored from seed-coral-cove

Data age: 8 hours (snapshot from 02:00, now 10:00)
```

Recovery works. You lost 8 hours of data (the time since the last backup), but the service is running.

---

### The Recovery Runbook

You document your recovery procedures:

```markdown
# Zen Garden Disaster Recovery Runbook

## Scenario: Single Stone Failure

1. Identify failed Stone:
   garden-rake observe

2. If offerings can run elsewhere, vacate first (if Stone recoverable):
   garden-rake vacate stone-name to another-stone

3. If Stone unrecoverable, restore offerings to new/different Stone:
   garden-rake restore mongodb from seed-bank seed-amber-brook on stone-bronze-canyon

4. Update DNS/load balancers if needed (garden discovery handles most cases)

## Scenario: Seed Bank Failure

1. Check remaining seed banks:
   garden-rake show seed-banks

2. If backups exist elsewhere, no immediate action needed

3. Replace failed seed bank:
   garden-rake prepare seed-bank /dev/sdX named new-seed-bank

4. Trigger full backup to new seed bank:
   garden-rake nurturing trigger-all

## Scenario: Complete Garden Loss

1. Install Moss on new hardware
2. Attach seed bank with backups
3. Restore Stone identity (if available):
   garden-rake restore stone-identity from seed-bank
4. Restore offerings one by one:
   garden-rake restore mongodb from seed-bank
   garden-rake restore postgres from seed-bank
   ...

## Recovery Time Objectives

| Offering | Max Data Loss | Recovery Time |
|----------|---------------|---------------|
| mongodb | 6 hours | 15 minutes |
| postgres | 6 hours | 20 minutes |
| redis | 6 hours | 5 minutes |
| elasticsearch | 6 hours | 30 minutes |
| ollama | 24 hours | 45 minutes |

## Contacts

- Primary: your-email@example.com
- Backup: colleague@example.com
```

---

### Scheduling Regular Tests

You set a reminder: test recovery quarterly. Not the whole garden—just one service at a time.

```
Q1: Test MongoDB recovery from seed bank
Q2: Test Postgres recovery to different Stone
Q3: Test Stone-level recovery (all offerings)
Q4: Test seed bank failure (remove one, verify other works)
```

Untested backups are not backups. They're hopes.

---

## What Just Happened

### The Backup Hierarchy

Your garden has three levels of backup:

```
┌─────────────────────────────────────────────────────────────────┐
│  BACKUP HIERARCHY                                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Level 1: LOCAL A/B SLOTS                                       │
│  ├─ On Stone's local disk                                       │
│  ├─ Two slots per offering (rotation)                           │
│  ├─ Fast restore (no network transfer)                          │
│  └─ Lost if Stone disk fails                                    │
│                                                                 │
│  Level 2: SEED BANKS                                            │
│  ├─ External USB storage                                        │
│  ├─ Multiple snapshots per offering (default: 5)                │
│  ├─ Survives Stone failure                                      │
│  └─ Can be physically moved to different location               │
│                                                                 │
│  Level 3: CROSS-STONE REPLICATION (future)                      │
│  ├─ Snapshots copied to other Stones                            │
│  ├─ Network-based redundancy                                    │
│  └─ Survives seed bank and Stone failure                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

For home labs, Level 1 and 2 usually provide sufficient protection. The seed bank is your disaster recovery lifeline.

### What the Seed Bank Contains

```
seed-amber-brook/.zen-garden/
├── manifest.yaml              # Seed bank identity
├── garden/
│   ├── index.yaml             # Inventory of all backups
│   └── offerings/
│       ├── mongodb/
│       │   ├── 2026-02-15T02-00-00/
│       │   │   ├── manifest.yaml    # Metadata, checksums
│       │   │   └── data.archive.gz  # Compressed volume data
│       │   ├── 2026-02-14T02-00-00/
│       │   └── ...
│       ├── postgres/
│       └── ...
└── journal/
    └── sync-history.yaml      # Replication history
```

Each offering has multiple timestamped snapshots. The manifest tracks checksums for integrity verification.

### Offering Identity Preservation

Offerings have a stable ID (UUIDv7) that survives:
- Renames (`mongodb` → `prod-mongodb`)
- Moves between Stones
- Restores from backup

This means the garden knows that the MongoDB you restored is the *same* MongoDB that was backed up, even if it's now on a different Stone with a different name.

### Recovery Scenarios

| Scenario | Data Loss | Recovery Method |
|----------|-----------|-----------------|
| Container crash | None | Auto-restart |
| Bad update | None | Rollback from harvest |
| Stone reboot | None | Containers auto-start |
| Stone disk fails | Since last backup | Restore from seed bank |
| Stone destroyed | Since last backup | Restore to new Stone |
| Seed bank fails | None (if redundant) | Other seed bank |
| All seed banks fail | Since last local backup | Local A/B slots |
| Everything fails | Everything | Rebuild from scratch |

The goal isn't zero data loss—that requires more sophisticated (and expensive) solutions. The goal is **bounded, predictable data loss** with **clear recovery procedures**.

---

## The Preparation Checklist

**Hardware:**
- [ ] At least one seed bank attached
- [ ] Seed bank larger than total offering data
- [ ] Consider second seed bank for redundancy
- [ ] Keep one seed bank off-site (if critical)

**Configuration:**
- [ ] All offerings have backup schedules
- [ ] Large offerings (like AI models) included
- [ ] No `latest` tags in production
- [ ] Retention policies appropriate for your needs

**Documentation:**
- [ ] Recovery runbook written
- [ ] Contact list current
- [ ] RTO/RPO defined per service

**Testing:**
- [ ] Quarterly recovery tests scheduled
- [ ] At least one full restore tested
- [ ] Seed bank restoration tested

**Monitoring:**
- [ ] Backup success alerts
- [ ] Seed bank capacity alerts
- [ ] Backup age warnings (>24h without backup)

---

## Commands From This Journey

```bash
# Full garden inventory
garden-rake observe --verbose

# List backups for an offering
garden-rake nurturing list mongodb

# Trigger manual backup
garden-rake nurturing trigger mongodb

# Trigger backup of all offerings
garden-rake nurturing trigger-all

# Restore from local slot
garden-rake restore mongodb from slot A

# Restore from seed bank
garden-rake restore mongodb from seed-bank seed-amber-brook

# Restore to different Stone
garden-rake restore mongodb from seed-bank seed-amber-brook on stone-bronze-canyon

# Pin offering to specific version
garden-rake pin minio to RELEASE.2024-01-28T22-35-53Z

# Show seed bank status
garden-rake show seed-banks

# Prepare new seed bank
garden-rake prepare seed-bank /dev/sdb named backup-drive
```

---

*Zen Garden Documentation — Journeys*
