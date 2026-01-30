# The Reconciliation

*Things diverged. Now they must converge.*

---

## The Story

You have three Stones. They've been running independently for a week—your router failed, and each Stone has been operating in isolation. Each thought it was the only one left.

Now the network is fixed. The Stones can see each other again. But their states have diverged.

Time for reconciliation.

---

### The Divergence

When the network comes back, the first Stone to notice alerts you:

```
⚠ TOPOLOGY CONFLICT DETECTED

  Multiple Stones have been operating independently:

  stone-amber-ridge (this Stone):
    • Offline since: 2026-03-15 09:00
    • Changes during isolation:
      - Started grafana (wasn't running before)
      - Updated redis 7.2.7 → 7.2.8

  stone-coral-reef:
    • Offline since: 2026-03-15 09:00
    • Changes during isolation:
      - Started redis (duplicate!)
      - Removed nginx

  stone-bronze-canyon:
    • Offline since: 2026-03-15 09:00
    • Changes during isolation:
      - No changes

  Conflicts requiring resolution:
    • redis running on both stone-amber-ridge AND stone-coral-reef

  Run 'garden-rake reconcile' to resolve conflicts.
```

Each Stone made changes while isolated. Now there's a conflict: Redis is running on two Stones.

---

### Starting Reconciliation

```bash
garden-rake reconcile
```

```
╔══════════════════════════════════════════════════════════════════╗
║                     GARDEN RECONCILIATION                         ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  Your garden has diverged. This wizard will help resolve         ║
║  conflicts and merge changes from the isolation period.          ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝

Step 1 of 3: Review Non-Conflicting Changes

  These changes will be accepted automatically:

  stone-amber-ridge:
    ✓ Started grafana
    ✓ Updated redis 7.2.7 → 7.2.8

  stone-coral-reef:
    ✓ Removed nginx

  stone-bronze-canyon:
    (no changes)

  Accept these changes? [Y/n] y
```

The non-conflicting changes merge automatically. Grafana started, nginx removed, redis updated—these don't conflict with anything.

---

### Resolving Conflicts

```
Step 2 of 3: Resolve Conflicts

  CONFLICT: redis running on multiple Stones

  During isolation:
    • stone-amber-ridge started redis (was already running here before isolation)
    • stone-coral-reef started redis (new instance)

  Options:
    [1] Keep stone-amber-ridge version (original, updated to 7.2.8)
    [2] Keep stone-coral-reef version (newer instance)
    [3] Keep both (run as balanced service)
    [4] Manual inspection

  Selection: 1

  Resolving...
    Stopping redis on stone-coral-reef... done
    Removing duplicate container... done
    ✓ redis consolidated on stone-amber-ridge
```

You chose to keep the original instance—it has the history and was updated during isolation.

---

### Finalizing

```
Step 3 of 3: Verify Reconciled State

  Current garden state:

  stone-amber-ridge:
    ● mongodb        Running    (unchanged)
    ● redis          Running    (updated 7.2.7 → 7.2.8)
    ● grafana        Running    (newly started)

  stone-coral-reef:
    ● postgres       Running    (unchanged)
    (nginx removed)
    (redis removed - duplicate)

  stone-bronze-canyon:
    ● elasticsearch  Running    (unchanged)

  Verify this matches your expectations.

  Finalize reconciliation? [Y/n] y

  Broadcasting reconciled state to all Stones... done
  Updating topology caches... done
  Syncing seed bank journals... done

✓ Garden reconciled

  All Stones are now synchronized.
  Seed banks have recorded the reconciliation event.
```

The garden is whole again.

---

### A Week Later: Backup Conflicts

You restore a Stone from a seed bank backup. But the backup is from before some changes happened:

```bash
garden-rake restore stone-bronze-canyon from seed-bank seed-amber-brook
```

```
Restoring stone-bronze-canyon from seed-bank...

  ⚠ Backup Timestamp Conflict

  The backup is from 2026-03-10, but the garden has changes from after that date:

  Changes since backup:
    • 2026-03-12: mongodb updated 7.0.8 → 8.0.0
    • 2026-03-14: redis configuration changed
    • 2026-03-15: grafana added

  The backup predates these changes. Restoring will:
    • Bring back mongodb 7.0.8 (older version)
    • Restore old redis configuration
    • NOT include grafana

  Options:
    [1] Restore anyway (you'll need to manually re-apply changes)
    [2] Restore then re-sync from current garden state
    [3] Cancel

  Selection: 2
```

You choose to restore and then re-sync. The garden will bring the restored Stone up to current state:

```
Restoring from backup...
  Extracting archives... done
  Recreating containers... done
  ✓ Backup restored

Re-syncing with current garden state...

  Offerings to update:
    mongodb: 7.0.8 → 8.0.0
      Pulling new image... done
      Applying update... done
      ✓ Updated

  Configuration to sync:
    redis: Applying current configuration
      ✓ Synced

  Missing offerings:
    grafana: Not in backup, exists in garden
      Installing... done
      ✓ Added

✓ stone-bronze-canyon restored and synchronized

The Stone now matches the current garden state.
```

---

### The Manual Conflict

Sometimes conflicts can't be auto-resolved:

```bash
garden-rake reconcile
```

```
CONFLICT: Data divergence in mongodb

  stone-amber-ridge mongodb has:
    • 15,342 documents in 'inventory' collection
    • Last write: 2026-03-20 14:30:00

  stone-bronze-canyon mongodb has:
    • 15,128 documents in 'inventory' collection
    • Last write: 2026-03-20 14:35:00

  Both instances have unique writes that the other doesn't have.
  This cannot be automatically resolved.

  Options:
    [1] Keep stone-amber-ridge data (lose 214 documents from stone-bronze-canyon)
    [2] Keep stone-bronze-canyon data (lose 428 documents from stone-amber-ridge)
    [3] Export both and merge manually
    [4] Contact application maintainer

  Selection: 3

  Exporting data...
    Exporting stone-amber-ridge mongodb... done
      → /tmp/reconcile/mongodb-amber-ridge.archive

    Exporting stone-bronze-canyon mongodb... done
      → /tmp/reconcile/mongodb-bronze-canyon.archive

  Archives saved. Use your application's merge tools to reconcile data,
  then import the merged data with:
    garden-rake import mongodb from /path/to/merged/data
```

Data conflicts require human judgment. The garden helps you export and re-import, but won't guess which data to keep.

---

## What Just Happened

### Why Conflicts Occur

Zen Garden is designed for availability—Stones continue operating even when isolated. This means:

```
Normal operation:
  Stone A ←→ Stone B ←→ Stone C
  All see same state
  Changes propagate instantly

Network partition:
  Stone A    |    Stone B    |    Stone C
  (isolated) |   (isolated)  |   (isolated)

Each Stone:
  • Continues serving requests
  • Allows local changes
  • Maintains local state

Network heals:
  Stone A ←→ Stone B ←→ Stone C
  States must merge
  Conflicts possible
```

This is an explicit trade-off: the garden prioritizes availability over consistency. You can always use it, even during network failures.

### Conflict Types

| Type | Cause | Resolution |
|------|-------|------------|
| **Duplicate offering** | Same offering started on multiple Stones | Choose one, remove others |
| **Version mismatch** | Same offering updated to different versions | Choose version, update others |
| **Configuration drift** | Settings changed differently | Merge or choose one |
| **Data divergence** | Application data written to different instances | Manual merge required |
| **Membership changes** | Stones joined/left during partition | Accept cumulative changes |

### The Reconciliation Protocol

```
┌─────────────────────────────────────────────────────────────────┐
│  RECONCILIATION PROTOCOL                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: DISCOVERY                                             │
│  ├─ Exchange timestamps of last common state                    │
│  ├─ Each Stone sends change log since then                      │
│  └─ Build complete picture of divergence                        │
│                                                                 │
│  Phase 2: CLASSIFICATION                                        │
│  ├─ Non-conflicting: Apply automatically                        │
│  │   (additions, independent updates)                           │
│  ├─ Soft conflicts: Apply with rules                            │
│  │   (version updates, config changes)                          │
│  └─ Hard conflicts: Require human decision                      │
│      (duplicate offerings, data divergence)                     │
│                                                                 │
│  Phase 3: RESOLUTION                                            │
│  ├─ Apply automatic changes                                     │
│  ├─ Present soft conflicts with recommendations                 │
│  └─ Present hard conflicts with options                         │
│                                                                 │
│  Phase 4: COMMITMENT                                            │
│  ├─ Broadcast reconciled state to all Stones                    │
│  ├─ Each Stone applies resolved state                           │
│  └─ Record reconciliation in seed bank journal                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Conflict Resolution Rules

The garden applies these rules to soft conflicts:

**Version conflicts:**
- Higher version wins (for same offering)
- User prompted if versions are incompatible (e.g., 7.x vs 8.x)

**Configuration conflicts:**
- Later timestamp wins (most recent change)
- User prompted if both changed same setting

**Duplicate offerings:**
- Original location preferred
- User prompted to choose if no clear original

**Membership:**
- Cumulative (all joins and leaves apply)
- Revocations always apply

### The Change Log

Each Stone maintains a change log during operation:

```yaml
# /var/lib/zen-garden/state/changelog.yaml

entries:
  - timestamp: 2026-03-15T09:00:00Z
    type: network_partition
    description: "Lost contact with garden"

  - timestamp: 2026-03-15T09:15:00Z
    type: offering_start
    offering: grafana
    stone: stone-amber-ridge
    reason: "Manual start during isolation"

  - timestamp: 2026-03-17T02:00:00Z
    type: offering_update
    offering: redis
    from_version: 7.2.7
    to_version: 7.2.8
    reason: "Scheduled nourishment"

  - timestamp: 2026-03-22T14:00:00Z
    type: network_restore
    description: "Garden topology restored"
```

The changelog is compared during reconciliation to understand what happened on each Stone.

### Preventing Conflicts

You can reduce conflicts by:

1. **Designating a coordinator**: One Stone handles changes during partitions
   ```bash
   garden-rake configure --coordinator stone-amber-ridge
   ```

2. **Enabling strict mode**: Reject changes during isolation
   ```bash
   garden-rake configure --strict-mode
   ```
   (Trade: reduced availability during partitions)

3. **Using balanced services**: Run offerings on multiple Stones intentionally
   ```bash
   garden-rake grow redis --balanced
   ```
   (No conflict if redis was already on multiple Stones)

Most home gardens don't need these—conflicts are rare and easy to resolve.

### Seed Bank Journal

The seed bank records reconciliation events:

```yaml
# seed-bank/.zen-garden/journal/reconciliation-log.yaml

events:
  - timestamp: 2026-03-22T14:30:00Z
    type: reconciliation
    trigger: network_restored
    duration: 3m 42s
    stones_involved: 3
    changes_merged: 4
    conflicts_resolved: 1
    resolution_choices:
      - conflict: duplicate_offering
        offering: redis
        choice: kept_stone_amber_ridge
        reason: "User selection: original instance"
```

This provides audit trail for what happened and why.

---

## Commands From This Journey

```bash
# Start reconciliation wizard
garden-rake reconcile

# Check for pending conflicts
garden-rake status --conflicts

# View change log
garden-rake changelog
garden-rake changelog --since 2026-03-15

# Force sync to current garden state
garden-rake sync stone-bronze-canyon

# Export offering data for manual merge
garden-rake export mongodb --to /path/to/export/

# Import merged data
garden-rake import mongodb from /path/to/merged/

# Configure coordinator Stone
garden-rake configure --coordinator stone-name

# Enable strict mode (reject changes during partition)
garden-rake configure --strict-mode

# View reconciliation history
garden-rake show reconciliations

# Dry-run reconciliation (see what would happen)
garden-rake reconcile --dry-run
```

---

*Zen Garden Documentation — Journeys*
