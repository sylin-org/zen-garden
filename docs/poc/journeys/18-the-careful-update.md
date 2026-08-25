# The Careful Update

*MongoDB 8.0 is a major version. You want to be careful.*

---

## The Story

Your morning nourishment check shows something significant:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 3 available, 0 blocked

───────────────────────────────────────────────────────────────

  stone-amber-ridge
    AVAILABLE:
      • mongodb 7.0.8 → 8.0.0 (major version)
      • redis 7.2.6 → 7.2.7

  stone-coral-reef
    AVAILABLE:
      • postgres 16.2 → 16.3

───────────────────────────────────────────────────────────────
```

Redis and Postgres are minor patches. You'd apply those without much thought. But MongoDB 8.0? That's a major version jump. New features, potential breaking changes, schema requirements.

You press Q. Not today. First, research.

---

A week later, you've read the release notes. Tested on a dev instance. You're ready. But you want to be methodical.

```bash
garden-rake nourish mongodb --dry-run
```

```
Dry run: Nourishing mongodb on stone-amber-ridge

Would execute:
  1. COLLECT (create harvest)
     - Commit container state
     - Archive /data/db (estimated: 4.2 GB)
     - Save manifest with rollback info

  2. NOURISH (apply update)
     - Pull mongo:8.0.0 (estimated: 850 MB)
     - Stop current container
     - Create new container with updated image
     - Mount existing volumes

  3. WATER (verify)
     - Start container
     - Wait for health check (mongosh --eval "db.runCommand('ping')")
     - If fails: automatic rollback to mongo:7.0.8

Estimated time: 5-8 minutes
Estimated disk for harvest: 4.5 GB

No changes made (dry run).
```

The garden shows you exactly what will happen. Three phases. Automatic rollback if the health check fails. Now you know the plan.

---

Saturday morning. Low traffic. You run the update:

```bash
garden-rake nourish mongodb
```

```
Nourishing mongodb on stone-amber-ridge...

  Phase 1: COLLECT
    Quiescing database (db.fsyncLock)... done
    Committing container state... done
    Archiving volumes (4.2 GB)... done
    Resuming database (db.fsyncUnlock)... done
    ✓ Harvest created: mongodb-20260201T093000-a1b2

  Phase 2: NOURISH
    Pulling image mongo:8.0.0... done (850 MB)
    Stopping current container... done
    Removing old container... done
    Creating new container... done
    Mounting volumes... done
    ✓ Container updated

  Phase 3: WATER
    Starting container... done
    Waiting for health check...
```

You watch. The health check runs. MongoDB starts. The authentication system initializes. The replica set (if any) syncs.

```
    Health check: mongosh --eval "db.runCommand('ping')"
    Attempt 1/3... passed

  ✓ mongodb updated: 7.0.8 → 8.0.0

Harvest preserved: mongodb-20260201T093000-a1b2
  Expires: 7 days
  Use 'garden-rake restore mongodb from harvest' to rollback manually.
```

Success. MongoDB 8.0 is running. The harvest—your pre-update snapshot—is preserved for a week in case you need to roll back.

---

You verify the update:

```bash
garden-rake status mongodb
```

```
mongodb on stone-amber-ridge

  Status: Running
  Health: Healthy
  Image: mongo:8.0.0 (updated 2 minutes ago)
  Port: 27017
  Uptime: 2m 15s

  Last Update:
    From: mongo:7.0.8
    At: 2026-02-01T09:30:00Z
    Harvest: mongodb-20260201T093000-a1b2 (available for 7d)

  Volumes:
    /data/db → /var/lib/zen-garden/offerings/mongodb/data
```

Your application connects. Queries work. Everything looks good.

---

Monday morning. A developer reports an issue: a query that worked before now fails. MongoDB 8.0 changed some aggregation behavior.

You have options:

**Option 1: Fix the application**
The "right" answer. Update the query to work with MongoDB 8.0.

**Option 2: Rollback temporarily**
Buy time while you fix the application. The harvest is still there.

You choose option 2—rollback while you investigate:

```bash
garden-rake restore mongodb from harvest
```

```
Restoring mongodb from harvest mongodb-20260201T093000-a1b2...

  Stopping current container... done
  Removing current container... done
  Restoring volumes from archive (4.2 GB)... done
  Restoring original image (mongo:7.0.8)... done
  Creating container... done
  Starting container... done
  Verifying health... passed

✓ mongodb restored to 7.0.8

Note: The 8.0.0 update is still available via 'garden-rake nourish'.
```

You're back on MongoDB 7.0.8. The developer's query works again. You have time to fix the application properly before attempting the upgrade again.

---

## What Just Happened

### The Three-Phase Ceremony

Every nourishment follows three phases:

```
┌─────────────────────────────────────────────────────────────────┐
│  NOURISHMENT CEREMONY                                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: COLLECT                                               │
│  ├─ Quiesce if supported (freeze writes)                       │
│  ├─ Commit container state                                      │
│  ├─ Archive volumes with checksums                              │
│  ├─ Resume if quiesced                                          │
│  └─ Save harvest manifest                                        │
│                                                                 │
│  Phase 2: NOURISH                                               │
│  ├─ Pull new image                                              │
│  ├─ Stop old container                                          │
│  ├─ Remove old container                                        │
│  └─ Create new container with new image                         │
│                                                                 │
│  Phase 3: WATER                                                 │
│  ├─ Start new container                                         │
│  ├─ Run health check                                            │
│  │   ├─ If PASS: Complete ceremony                              │
│  │   └─ If FAIL: Automatic rollback                             │
│  └─ Clean up or preserve harvest                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

If anything fails in phases 2 or 3, the garden automatically rolls back using the harvest from phase 1.

### Ceremony Modes

Different offerings require different snapshot strategies:

| Mode | Behavior | Example |
|------|----------|---------|
| `stateless` | No quiesce needed, safe anytime | nginx, web servers |
| `quiesceable` | Freeze/thaw without stopping | MongoDB, PostgreSQL |
| `unsafe` | Must stop before snapshot | Unknown applications |

MongoDB is quiesceable—it can freeze writes (`db.fsyncLock`), let you snapshot, then resume (`db.fsyncUnlock`) without stopping the service entirely.

### The Harvest

A harvest contains everything needed to restore:

```
/var/lib/zen-garden/harvests/mongodb-20260201T093000-a1b2/
├── manifest.json       # Metadata, checksums, expiration
└── volumes/
    └── data-db.tar.zst # Compressed volume archive
```

The manifest includes:
- Original image reference (`mongo:7.0.8`)
- Volume checksums (BLAKE3)
- Creation timestamp
- Expiration time (default: 7 days)
- Ceremony ID that created it

### Automatic Rollback

If the health check fails after update:

```
  Phase 3: WATER
    Starting container... done
    Waiting for health check...
    Attempt 1/3... failed (timeout)
    Attempt 2/3... failed (connection refused)
    Attempt 3/3... failed (auth error)

  ⚠ Health check failed. Rolling back...

    Stopping failed container... done
    Restoring from harvest... done
    Starting restored container... done
    Verifying health... passed

  ✓ Rollback complete: mongodb remains at 7.0.8

  The update to 8.0.0 failed. Check the release notes for:
    - Breaking changes
    - Migration requirements
    - Configuration updates
```

You never have to manually intervene if an update breaks. The garden automatically detects the failure and restores the previous version.

### Manual vs Automatic

**Automatic updates** (the morning check):
- Shows what's available
- You choose when to apply
- Garden handles the ceremony

**Manual ceremony control**:
```bash
# Preview without executing
garden-rake nourish mongodb --dry-run

# Skip the harvest (dangerous, faster)
garden-rake nourish mongodb recklessly

# Update but keep harvest longer
garden-rake nourish mongodb --preserve-harvest 30d

# Restore from specific harvest
garden-rake restore mongodb from mongodb-20260201T093000-a1b2

# List available harvests
garden-rake harvests mongodb
```

### Staged Updates Across Stones

For critical services running on multiple Stones, update one at a time:

```bash
# Check which Stones have MongoDB
garden-rake find mongodb --all

# Update Stone A first
garden-rake nourish mongodb on stone-amber-ridge

# Verify, then update Stone B
garden-rake nourish mongodb on stone-bronze-canyon
```

This manual staging lets you verify each update before proceeding. If Stone A fails, Stone B is still serving traffic.

---

## The Update Discipline

**For minor patches** (7.2.6 → 7.2.7):
- Usually safe to apply during morning check
- Quick review of changelog
- Automatic rollback catches problems

**For major versions** (7.0 → 8.0):
- Read the release notes
- Test in development first
- Schedule during low-traffic window
- Use `--dry-run` to preview
- Keep the harvest longer than default
- Have application rollback plan ready

The garden makes updates safe. But safe doesn't mean careless. Major versions deserve careful attention.

---

## Commands From This Journey

```bash
# Preview update without executing
garden-rake nourish mongodb --dry-run

# Update specific offering
garden-rake nourish mongodb

# Update specific offering on specific Stone
garden-rake nourish mongodb on stone-amber-ridge

# Update without creating harvest (dangerous)
garden-rake nourish mongodb recklessly

# Keep harvest longer than default
garden-rake nourish mongodb --preserve-harvest 30d

# List available harvests
garden-rake harvests mongodb

# Restore from most recent harvest
garden-rake restore mongodb from harvest

# Restore from specific harvest
garden-rake restore mongodb from mongodb-20260201T093000-a1b2

# Delete old harvests manually
garden-rake harvests prune --older-than 7d
```

---

*Zen Garden Documentation — Journeys*
