# The Quarterly Prune

*Gardens need maintenance. So does yours.*

---

## The Story

Three months pass. Your garden runs smoothly. Services start, stop, update. Backups happen nightly. Everything works.

But things accumulate. Old Docker images from updates you've long since applied. Harvests from ceremonies months ago. Log files growing. Disk space slowly shrinking.

Time to prune.

---

### Checking the State

```bash
garden-rake status stone-amber-ridge --storage
```

```
stone-amber-ridge Storage Overview

  Disk: 256 GB total, 180 GB used (70%)

  Breakdown:
    Docker images:        45 GB
      ├─ In use:          12 GB (8 images)
      └─ Unused:          33 GB (23 images)

    Offering data:        95 GB
      ├─ mongodb:         42 GB
      ├─ redis:           3 GB
      └─ grafana:         50 GB

    Harvests:             18 GB
      ├─ Active:          4 GB (2 harvests, < 7 days)
      └─ Expired:         14 GB (8 harvests, > 7 days)

    Local backups (A/B):  15 GB
    Logs:                 7 GB

  Recommendations:
    • Prune unused Docker images: recover 33 GB
    • Remove expired harvests: recover 14 GB
    • Consider log rotation: 7 GB accumulated
```

70% disk usage. Not critical, but the unused images and expired harvests are wasting space.

---

### Pruning Docker Images

Every time you update an offering, the old image stays around. After months of updates, you have layers of history:

```bash
garden-rake prune images on stone-amber-ridge
```

```
Analyzing Docker images on stone-amber-ridge...

  Images in use (will keep):
    mongo:8.0.0          850 MB    mongodb
    redis:7.2.7          150 MB    redis
    grafana/grafana:10.3.1  400 MB    grafana
    ...

  Images not in use (will remove):
    mongo:7.0.8          820 MB    (previous version)
    mongo:7.0.5          815 MB    (previous version)
    mongo:6.0.12         780 MB    (old version)
    redis:7.2.6          148 MB    (previous version)
    redis:7.2.5          147 MB    (previous version)
    ...
    23 images, 33 GB total

Remove unused images? [y/N] y

  Removing mongo:7.0.8... done
  Removing mongo:7.0.5... done
  Removing mongo:6.0.12... done
  ...

✓ Removed 23 images, recovered 33 GB
```

The garden keeps images that are actively running. Everything else goes.

---

### Cleaning Expired Harvests

Harvests are snapshots created before updates. They're kept for rollback, but they have an expiration:

```bash
garden-rake harvests prune
```

```
Analyzing harvests on stone-amber-ridge...

  Active harvests (will keep):
    mongodb-20260315T093000-a1b2    4.2 GB    Expires: 2026-03-22
    redis-20260312T140000-c3d4      512 MB    Expires: 2026-03-19

  Expired harvests (will remove):
    mongodb-20260201T093000-e5f6    4.1 GB    Expired: 2026-02-08
    mongodb-20260115T020000-g7h8    4.0 GB    Expired: 2026-01-22
    postgres-20260101T020000-i9j0   8.1 GB    Expired: 2026-01-08
    ...
    8 harvests, 14 GB total

Remove expired harvests? [y/N] y

  Removing mongodb-20260201T093000-e5f6... done
  Removing mongodb-20260115T020000-g7h8... done
  ...

✓ Removed 8 harvests, recovered 14 GB
```

You could have rolled back to these—but you didn't need to. The offerings are stable. The harvests served their purpose.

---

### Log Rotation

Moss and its offerings generate logs. They grow over time:

```bash
garden-rake logs status on stone-amber-ridge
```

```
Log Status on stone-amber-ridge

  System logs:
    /var/log/zen-garden/moss.log       2.1 GB (90 days)
    /var/log/zen-garden/events.log     1.8 GB (90 days)

  Offering logs:
    mongodb:      1.5 GB (Docker logs, 90 days)
    redis:        0.3 GB (Docker logs, 90 days)
    grafana:      1.3 GB (Docker logs, 90 days)

  Total: 7 GB

  Recommendation: Configure log rotation or prune logs older than 30 days
```

You configure rotation for the future:

```bash
garden-rake logs configure --max-age 30d --max-size 500MB
```

```
Configuring log rotation...

  Docker daemon: max-size=500MB, max-file=3
  Moss logs: rotate daily, keep 30 days

✓ Log rotation configured

To immediately prune old logs:
  garden-rake logs prune --older-than 30d
```

And clean up the current accumulation:

```bash
garden-rake logs prune --older-than 30d
```

```
Pruning logs older than 30 days...

  Removed: 5.2 GB of logs

  Remaining:
    System logs: 1.0 GB
    Offering logs: 0.8 GB
```

---

### Seed Bank Maintenance

Your seed banks also accumulate:

```bash
garden-rake show seed-banks --verbose
```

```
Seed Banks

  seed-amber-brook (USB, 64 GB)
    Mounted on: stone-amber-ridge
    Used: 58 GB (91%)

    Snapshots by offering:
      mongodb:        25 GB (15 snapshots)
      redis:          3 GB (15 snapshots)
      postgres:       20 GB (15 snapshots)
      grafana:        10 GB (15 snapshots)

    ⚠ 91% full. Consider pruning old snapshots.

  seed-coral-cove (USB, 128 GB)
    Mounted on: stone-coral-reef
    Used: 67 GB (52%)
    ...
```

The default retention is 5 snapshots per offering. But you have 15. Time to enforce the policy:

```bash
garden-rake nurturing prune-seed-bank seed-amber-brook --keep 5
```

```
Pruning seed-amber-brook to keep 5 snapshots per offering...

  mongodb: Removing 10 old snapshots (15 GB)
  redis: Removing 10 old snapshots (1.5 GB)
  postgres: Removing 10 old snapshots (12 GB)
  grafana: Removing 10 old snapshots (6 GB)

✓ Recovered 34.5 GB on seed-amber-brook
  New usage: 23.5 GB (37%)
```

---

### The Quarterly Checklist

You create a recurring reminder with this checklist:

```markdown
# Quarterly Garden Maintenance

## Cleanup
- [ ] Prune unused Docker images on each Stone
- [ ] Remove expired harvests
- [ ] Prune seed bank snapshots beyond retention
- [ ] Check and rotate logs

## Health Check
- [ ] Review disk usage on each Stone (<80% target)
- [ ] Check seed bank capacity (<80% target)
- [ ] Verify backup schedules are running
- [ ] Test one restore from seed bank

## Updates
- [ ] Review available updates (garden-rake nourish)
- [ ] Apply security patches
- [ ] Plan major version updates for next quarter

## Documentation
- [ ] Update recovery runbook if needed
- [ ] Review and update contact list
- [ ] Document any configuration changes
```

---

## What Just Happened

### Automatic vs Manual Maintenance

The garden handles some maintenance automatically:

| Task | Automatic | Manual |
|------|-----------|--------|
| Topology cache pruning | ✓ Every 30s | — |
| A/B slot rotation | ✓ On backup | — |
| Harvest expiration | ✓ After TTL | `harvests prune` |
| Docker image cleanup | — | `prune images` |
| Log rotation | ✓ If configured | `logs prune` |
| Seed bank pruning | — | `nurturing prune-seed-bank` |

The garden cleans up its internal state automatically. But Docker images and seed bank snapshots require explicit pruning—they might be wanted for rollback or historical reference.

### What Gets Pruned

**Docker Images:**
- Unused images (not running in any container)
- Dangling layers (no tag, no reference)
- Build cache (if present)
- Keeps: Currently running images, base images of running containers

**Harvests:**
- Expired harvests (past `expires_at` timestamp)
- Default TTL: 7 days
- Keeps: Active harvests within TTL

**Seed Bank Snapshots:**
- Oldest snapshots beyond retention count
- Default retention: 5 per offering
- Keeps: Most recent N snapshots per offering

**Logs:**
- Files older than specified age
- Files exceeding size limit (rotated)
- Keeps: Recent logs within retention window

### The Maintenance Rhythm

**Daily:** Automatic
- Topology pruning (30s)
- Backup rotation (A/B slots)
- Health monitoring (30s)

**Weekly:** Light touch
- Glance at disk usage
- Check backup success

**Monthly:** Moderate attention
- Review `nourish` for updates
- Check seed bank capacity

**Quarterly:** Full maintenance
- Prune images, harvests, logs
- Test recovery
- Update documentation

This isn't rigid—adjust to your environment. The goal is regular, predictable maintenance rather than crisis-driven cleanup.

---

## Commands From This Journey

```bash
# Check storage breakdown
garden-rake status stone-amber-ridge --storage

# Prune unused Docker images
garden-rake prune images on stone-amber-ridge

# Prune all Stones
garden-rake prune images --all

# List harvests
garden-rake harvests

# Prune expired harvests
garden-rake harvests prune

# Prune harvests older than specific age
garden-rake harvests prune --older-than 14d

# Check log status
garden-rake logs status on stone-amber-ridge

# Configure log rotation
garden-rake logs configure --max-age 30d --max-size 500MB

# Prune old logs
garden-rake logs prune --older-than 30d

# Show seed bank details
garden-rake show seed-banks --verbose

# Prune seed bank to retention policy
garden-rake nurturing prune-seed-bank seed-amber-brook --keep 5

# Full quarterly maintenance (interactive)
garden-rake maintain
```

---

*Zen Garden Documentation — Journeys*
