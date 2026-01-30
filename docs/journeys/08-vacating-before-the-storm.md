# Vacating Before the Storm

*The forecast says power outage. You have two hours.*

---

## The Story

Thursday afternoon. Your phone buzzes with an alert from the power company:

```
PLANNED OUTAGE NOTICE
Area: Your neighborhood
Time: Today, 6:00 PM - 10:00 PM
Reason: Transformer maintenance
```

Four hours without power. Your UPS can handle maybe 20 minutes. Stone-coral-reef—the thin client running Redis and Postgres—will go down hard.

You could just let it happen. The garden would mark the Stone offline, applications would fail over or error out, and everything would come back when power returns.

But you have two hours. You can do better.

---

Stone-amber-ridge is a laptop with a battery. It can survive the outage on battery power, or you can move it somewhere with power. Either way, it'll stay running.

You decide to move everything off stone-coral-reef to stone-amber-ridge.

```bash
garden-rake vacate stone-coral-reef to stone-amber-ridge
```

```
Planning evacuation of stone-coral-reef...

  Services to move:
    • redis (512 MB data)
    • postgres (2.1 GB data)

  Destination: stone-amber-ridge
    • Available memory: 4.2 GB (sufficient)
    • Available disk: 180 GB (sufficient)

Proceed with evacuation? [y/N] y

Evacuating stone-coral-reef...

  [1/2] Moving redis...
        Creating snapshot on stone-coral-reef
        Transferring to stone-amber-ridge (512 MB)
        Starting on stone-amber-ridge
        Health check passed
        Stopping on stone-coral-reef
        ✓ redis now on stone-amber-ridge

  [2/2] Moving postgres...
        Quiescing database (pg_start_backup)
        Creating snapshot on stone-coral-reef
        Transferring to stone-amber-ridge (2.1 GB)
        Starting on stone-amber-ridge
        Health check passed
        Stopping on stone-coral-reef
        ✓ postgres now on stone-amber-ridge

Evacuation complete.

  stone-coral-reef: 0 offerings (safe to power off)
  stone-amber-ridge: 3 offerings (mongodb, redis, postgres)
```

---

You check the garden:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Up 45d

   OFFERINGS:
   ├─ mongodb     Running   Healthy   27017
   ├─ redis       Running   Healthy   6379
   └─ postgres    Running   Healthy   5432

●  stone-coral-reef (192.168.1.58)
   Moss 0.2.1 • Up 23d

   OFFERINGS:
   (none)
```

All services are now on stone-amber-ridge. Stone-coral-reef is empty—safe to shut down.

You check your applications:

```
[17:45:23] Reconnecting to zen-garden:redis...
[17:45:23] Resolved zen-garden:redis → redis://192.168.1.42:6379
[17:45:23] Connected to Redis

[17:45:24] Reconnecting to zen-garden:postgresql...
[17:45:24] Resolved zen-garden:postgresql → postgresql://192.168.1.42:5432
[17:45:24] Connected to PostgreSQL
```

The applications reconnected automatically. They found the services at their new locations without any configuration changes.

---

5:55 PM. Five minutes before the outage.

```bash
garden-rake slumber stone-coral-reef
```

```
Shutting down stone-coral-reef...
  Sending shutdown command
  Waiting for graceful shutdown...
  ✓ Stone is offline

stone-coral-reef powered off gracefully.
```

You unplug the thin client. No data loss. No corruption. Just a clean shutdown.

---

10:15 PM. Power's back. You plug stone-coral-reef back in. It boots.

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   ├─ mongodb     Running   Healthy
   ├─ redis       Running   Healthy
   └─ postgres    Running   Healthy

●  stone-coral-reef (192.168.1.58)
   Moss 0.2.1 • Up 2m

   OFFERINGS:
   (none)
```

Stone-coral-reef is back, but empty. The services are still on stone-amber-ridge.

You could leave them there. But stone-coral-reef is plugged into the wall, while stone-amber-ridge is running on battery. Time to move things back:

```bash
garden-rake vacate stone-amber-ridge to stone-coral-reef --only redis,postgres
```

```
Evacuating redis, postgres from stone-amber-ridge...

  [1/2] Moving redis...
        ✓ redis now on stone-coral-reef

  [2/2] Moving postgres...
        ✓ postgres now on stone-coral-reef

Evacuation complete.
```

MongoDB stays on stone-amber-ridge. Redis and Postgres are back on stone-coral-reef. The garden is balanced again.

---

## What Just Happened

### The Vacate Ceremony

Vacate is a special ceremony for moving all services off a Stone. It's designed for planned maintenance:

```
┌─────────────────────────────────────────────────────────────────┐
│  VACATE CEREMONY                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: PLAN                                                  │
│  ├─ List all offerings on source Stone                         │
│  ├─ Check destination has sufficient resources                  │
│  ├─ Check destination compatibility (CPU features, etc.)        │
│  └─ Present plan for confirmation                               │
│                                                                 │
│  Phase 2: EVACUATE (per offering)                               │
│  ├─ Create snapshot on source                                   │
│  │   └─ Quiesce if stateful (db.fsyncLock, pg_start_backup)     │
│  ├─ Transfer snapshot to destination                            │
│  ├─ Start offering on destination                               │
│  ├─ Wait for health check                                       │
│  ├─ Stop offering on source                                     │
│  └─ Update topology (new location announced)                    │
│                                                                 │
│  Phase 3: VERIFY                                                │
│  ├─ Confirm all offerings running on destination                │
│  └─ Confirm source Stone has no offerings                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

The key insight: each service is moved atomically. Redis is stopped on the source only after it's healthy on the destination. If anything fails, the service stays where it was.

### The Transfer

Moving 2.1 GB of Postgres data takes time. Here's what happened:

```
Source (stone-coral-reef)          Destination (stone-amber-ridge)
─────────────────────────          ────────────────────────────────

1. Quiesce postgres
   "SELECT pg_start_backup('vacate')"
   (Postgres enters backup mode)

2. Create snapshot
   - Archive container state
   - Archive /var/lib/postgresql data
   (Postgres still running, serving reads)

3. Resume postgres
   "SELECT pg_stop_backup()"

4. Transfer snapshot ───────────────► Receive snapshot
   (2.1 GB over local network)         Write to local storage
   ~30 seconds at 70 MB/s

5.                                     Start postgres
                                       Wait for health check
                                       "SELECT 1" succeeds

6.                                     Announce: "I have postgres"

7. Stop postgres                       (Already serving traffic)
   Remove from registry

8. Announce: "I no longer have postgres"
```

During step 2-3, Postgres was in backup mode. It could serve reads but writes were paused. Total pause time: a few seconds.

During step 4-6, both Stones briefly had Postgres. The old one was still serving traffic while the new one started up.

After step 6, discovery returned the new location. Applications reconnected.

### Partial Vacate

You can move specific services instead of everything:

```bash
# Move only redis and postgres
garden-rake vacate stone-amber-ridge --only redis,postgres

# Move everything except mongodb
garden-rake vacate stone-amber-ridge --except mongodb

# Move to a specific destination
garden-rake vacate stone-amber-ridge to stone-coral-reef

# Let the garden choose the best destination
garden-rake vacate stone-amber-ridge --anywhere
```

With `--anywhere`, the garden evaluates all Stones and picks the best destination based on:
- Available resources (memory, disk)
- Current load
- Hardware compatibility
- Network proximity

### The Slumber Command

`garden-rake slumber` sends a graceful shutdown command:

```bash
garden-rake slumber stone-coral-reef
```

This runs `shutdown -h now` on the Stone (via SSH or Moss API). It's better than yanking the power cord because:

- Filesystems sync properly
- Docker containers get SIGTERM
- Databases can checkpoint
- No risk of corruption

For planned outages, always slumber before unplugging.

### The Rouse Command

The opposite of slumber is rouse—Wake-on-LAN:

```bash
garden-rake rouse stone-coral-reef
```

This sends a magic packet to the Stone's MAC address (remembered in the topology cache). If the Stone's BIOS supports Wake-on-LAN, it powers on.

```
10:00 PM: Power returns
10:01 PM: You run 'garden-rake rouse stone-coral-reef'
10:01 PM: Magic packet sent to 00:1A:2B:3C:4D:5E
10:02 PM: Stone boots, Moss starts, chirp sent
10:02 PM: Stone appears in garden
```

You don't have to physically press the power button.

---

## Planning Ahead

The power outage was announced in advance. Not all outages are. But you can prepare:

**For predictable events:**
- Vacate services to surviving Stones
- Slumber the affected Stone
- Power it back up when safe

**For unpredictable events:**
- Keep a seed bank updated (nightly snapshots)
- Run critical services on Stones with UPS or batteries
- Accept that some downtime is inevitable

**For critical services:**
- Run multiple instances on different Stones
- Use database replication (not just garden discovery)
- Consider geographic distribution (Lantern for cross-site)

The garden makes migration easy, but it doesn't make decisions for you. You know your environment: which Stones have battery backup, which services are critical, how much downtime is acceptable.

---

## Commands From This Journey

```bash
# Evacuate all services from a Stone
garden-rake vacate stone-coral-reef

# Evacuate to a specific destination
garden-rake vacate stone-coral-reef to stone-amber-ridge

# Evacuate specific services only
garden-rake vacate stone-coral-reef --only redis,postgres

# Evacuate everything except specific services
garden-rake vacate stone-coral-reef --except mongodb

# Let the garden choose destination
garden-rake vacate stone-coral-reef --anywhere

# Graceful shutdown
garden-rake slumber stone-coral-reef

# Wake-on-LAN
garden-rake rouse stone-coral-reef

# Reboot a Stone
garden-rake stir stone-coral-reef
```

---

*Zen Garden Documentation — Journeys*
