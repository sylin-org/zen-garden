# The Failed Update

*The health check didn't pass.*

---

## The Story

Monday morning. Coffee in hand. You run your weekly check:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 3 available, 0 blocked

  stone-amber-ridge
    AVAILABLE:
      • mongodb 7.0.5 → 7.0.8

  stone-coral-reef
    AVAILABLE:
      • redis 7.2.3 → 7.2.5
      • postgres 16.1 → 16.2
```

Three updates available. Nothing blocked. You decide to update MongoDB—it's been a few versions behind.

```bash
garden-rake nourish mongodb
```

```
Nourishing mongodb on stone-amber-ridge...

  [1/3] Collecting harvest...
        Quiescing database (fsyncLock)
        Archiving volumes (2.3 GB)
        Resuming database (fsyncUnlock)
        Harvest stored: /var/lib/zen-garden/harvests/mongodb-20260115-083042

  [2/3] Applying update...
        Pulling mongo:7.0.8
        Stopping mongodb
        Recreating container with new image
        Starting mongodb

  [3/3] Verifying health...
        Waiting for health check (attempt 1/5)...
        Waiting for health check (attempt 2/5)...
        Waiting for health check (attempt 3/5)...
        Waiting for health check (attempt 4/5)...
        Waiting for health check (attempt 5/5)...
        ✗ Health check failed after 5 attempts

  ⚠️  Nourishment failed. Rolling back...

        Stopping failed container
        Restoring from harvest
        Starting mongodb (7.0.5)
        Health check passed ✓

  Rolled back mongodb to 7.0.5
  Harvest preserved for investigation: mongodb-20260115-083042
```

The update failed. But MongoDB is still running—on the old version. Your applications didn't notice anything.

---

You investigate. What went wrong?

```bash
garden-rake watch mongodb logs --tail 50
```

```
2026-01-15T08:31:15.234Z E STORAGE  [initandlisten] WiredTiger error
  (-31802) __posix_open_file:811: /data/db/WiredTiger.wt:
  handle-open: open: Invalid argument

2026-01-15T08:31:15.235Z F STORAGE  [initandlisten] Failed to start up
  WiredTiger storage engine. See previous error for details.

2026-01-15T08:31:15.236Z F -        [initandlisten] Fatal Assertion 28595
```

A storage engine error. MongoDB 7.0.8 couldn't open the data files created by 7.0.5. Some incompatibility in the WiredTiger format.

This is exactly why the ceremony exists. If you had done a simple `docker pull && docker restart`, your database would be down right now. Instead, the garden:

1. Made a backup before touching anything
2. Tried the update
3. Detected the failure
4. Restored from backup automatically

Total downtime: zero. Data loss: zero.

---

You check the harvest:

```bash
ls /var/lib/zen-garden/harvests/
```

```
mongodb-20260115-083042/
  ├── manifest.yaml
  ├── container.tar.gz
  └── volumes/
      └── mongodb-data.tar.gz
```

The harvest contains everything: the container state, the volume data, metadata about the offering. If you needed to investigate further, it's all there.

You check the MongoDB release notes online. Turns out 7.0.8 requires a specific migration step for databases created before 7.0.6. The automatic update path doesn't work.

For now, you stay on 7.0.5. It's working fine. When you're ready to do the manual migration, the harvest will still be there as a safety net.

---

Later that day, you update Redis instead:

```bash
garden-rake nourish redis
```

```
Nourishing redis on stone-coral-reef...

  [1/3] Collecting harvest...
        Redis is stateless-cacheable, skipping volume backup
        Container state archived

  [2/3] Applying update...
        Pulling redis:7.2.5
        Stopping redis
        Recreating container
        Starting redis

  [3/3] Verifying health...
        Health check passed ✓

✓ Nourished redis: 7.2.3 → 7.2.5
```

Redis updated successfully. Notice the difference: "Redis is stateless-cacheable, skipping volume backup." The garden knows Redis is a cache—if it fails, you lose cache data, not primary data. The ceremony adapts.

---

A week later, you're feeling brave. You want to try the MongoDB update again, but this time you'll do it manually with the migration step.

First, you create an explicit backup:

```bash
garden-rake nurturing trigger mongodb
```

```
Creating backup of mongodb...
  Quiescing database
  Creating snapshot
  Storing to slot A

✓ Backup complete: slot A (2.3 GB)
```

Now you have two backups: the automatic harvest from the failed update, and this fresh manual backup in "slot A."

You do the migration (following MongoDB's documentation), then try the update:

```bash
garden-rake nourish mongodb
```

```
Nourishing mongodb on stone-amber-ridge...

  [1/3] Collecting harvest...
        Quiescing database
        Archiving volumes (2.4 GB)
        Resuming database
        Harvest stored

  [2/3] Applying update...
        Pulling mongo:7.0.8
        Stopping mongodb
        Recreating container
        Starting mongodb

  [3/3] Verifying health...
        Health check passed ✓

✓ Nourished mongodb: 7.0.5 → 7.0.8
```

This time it worked. The migration step fixed the compatibility issue.

---

## What Just Happened

### The Ceremony

An update in Zen Garden isn't just `docker pull`. It's a *ceremony*—a deliberate, multi-phase operation with safety guarantees.

```
┌─────────────────────────────────────────────────────────────────┐
│  NOURISHMENT CEREMONY                                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: COLLECT                                               │
│  ├─ Check offering template for ceremony mode                   │
│  ├─ If quiesceable: pause writes (fsyncLock, pg_start_backup)   │
│  ├─ Archive container state                                     │
│  ├─ Archive volume data (if stateful)                          │
│  ├─ If quiesced: resume writes                                  │
│  └─ Store as harvest                                            │
│                                                                 │
│  Phase 2: APPLY                                                 │
│  ├─ Pull new image                                              │
│  ├─ Stop current container                                      │
│  ├─ Create new container with same volumes                      │
│  └─ Start new container                                         │
│                                                                 │
│  Phase 3: VERIFY                                                │
│  ├─ Run health checks (up to 5 attempts)                        │
│  ├─ If healthy: ceremony complete ✓                            │
│  └─ If unhealthy: trigger rollback                              │
│                                                                 │
│  ROLLBACK (if verification fails)                               │
│  ├─ Stop failed container                                       │
│  ├─ Restore from harvest                                        │
│  ├─ Start restored container                                    │
│  ├─ Verify health                                               │
│  └─ Preserve harvest for investigation                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

The ceremony is designed to fail safely. If anything goes wrong in Phase 2 or 3, the harvest from Phase 1 is ready to restore.

### Ceremony Modes

Each offering template declares a ceremony mode:

```yaml
# mongodb template
name: mongodb
ceremony:
  mode: quiesceable
  quiesce: ["mongosh", "--eval", "db.fsyncLock()"]
  resume: ["mongosh", "--eval", "db.fsyncUnlock()"]

# redis template
name: redis
ceremony:
  mode: stateless-cacheable
  # No quiesce needed - data loss acceptable

# postgres template
name: postgres
ceremony:
  mode: quiesceable
  quiesce: ["psql", "-c", "SELECT pg_start_backup('nourish')"]
  resume: ["psql", "-c", "SELECT pg_stop_backup()"]
```

| Mode | Meaning | Backup Strategy |
|------|---------|-----------------|
| **stateless** | No persistent data | No volume backup |
| **stateless-cacheable** | Cache only, loss acceptable | Container state only |
| **quiesceable** | Can pause writes | Quiesce → backup → resume |
| **unsafe** | Can't pause, must stop | Stop → backup → start |

The garden uses this information to choose the safest approach for each offering.

### The Health Check

After applying the update, the garden runs health checks:

```yaml
# mongodb template
health:
  test: ["mongosh", "--eval", "db.runCommand('ping')"]
  interval: 10s
  timeout: 5s
  retries: 5
```

The ceremony waits for 5 successful health checks. If any fail, it waits and retries. After 5 failures, it declares the update failed and triggers rollback.

In your MongoDB case, the health check was:

```
mongosh --eval "db.runCommand('ping')"
```

This command tries to connect to MongoDB and run a simple ping. When the storage engine failed to start, the ping failed, and the ceremony knew something was wrong.

### The Harvest

The harvest is your safety net. It contains:

```
mongodb-20260115-083042/
├── manifest.yaml        # Metadata: version, ports, volumes, health config
├── container.tar.gz     # Docker container export
└── volumes/
    └── mongodb-data.tar.gz   # Volume data archive
```

The manifest records everything needed to recreate the offering:

```yaml
# manifest.yaml
offering: mongodb
version: 7.0.5
image: mongo:7.0.5
created_at: 2026-01-15T08:30:42Z
stone: stone-amber-ridge

volumes:
  - name: mongodb-data
    path: /data/db
    size_bytes: 2469606195

health:
  test: ["mongosh", "--eval", "db.runCommand('ping')"]
  interval: 10s

ceremony:
  mode: quiesceable
  quiesce_command: ["mongosh", "--eval", "db.fsyncLock()"]
  resume_command: ["mongosh", "--eval", "db.fsyncUnlock()"]
```

If you need to restore manually (maybe on a different Stone), the harvest has everything.

### Stateless vs. Stateful

Notice how Redis updated differently:

```
Redis is stateless-cacheable, skipping volume backup
```

Redis is a cache. If the update fails and data is lost, you lose cache entries—not primary data. Applications will repopulate the cache naturally.

The ceremony adapts:
- **Stateful offerings** (MongoDB, Postgres): Full volume backup, careful quiesce/resume
- **Cacheable offerings** (Redis, Memcached): Container state only, faster update
- **Stateless offerings** (nginx, API gateways): No backup needed, even faster

This isn't laziness—it's precision. Why spend minutes backing up a cache when the data is ephemeral anyway?

### The Reckless Modifier

Sometimes you want to skip the safety dance:

```bash
garden-rake nourish mongodb recklessly
```

This bypasses Phase 1 entirely. No harvest. No safety net. Just pull and restart.

Why would anyone do this?
- Development environments where data doesn't matter
- Stateless offerings that don't need backups anyway
- Emergency updates where speed matters more than safety
- You already have a backup from another source

The word "recklessly" is intentional. It makes you acknowledge you're taking a risk. The garden won't hide that from you.

---

## The Lesson

The MongoDB update failed. That's not a bug—that's the system working correctly.

In traditional infrastructure, a failed update means:
1. Your service is down
2. You scramble to figure out what went wrong
3. You manually restore from backup (if you have one)
4. You apologize to users for the outage

In Zen Garden, a failed update means:
1. The garden tried the update
2. It detected the failure automatically
3. It restored automatically
4. Your service kept running
5. You investigate at your leisure

The ceremony exists because updates fail. Not always, not often, but sometimes. When they do, the garden catches you.

---

## Commands From This Journey

```bash
# Check for available updates
garden-rake nourish

# Update specific offering
garden-rake nourish mongodb
garden-rake nourish redis

# Update without safety net (dangerous)
garden-rake nourish mongodb recklessly

# Manual backup before risky operation
garden-rake nurturing trigger mongodb

# Check backup status
garden-rake nurturing status mongodb

# Restore from backup slot
garden-rake restore mongodb from slot A

# View offering logs
garden-rake watch mongodb logs --tail 50
```

---

*Zen Garden Documentation — Journeys*
