# The Life of an Offering

*A journey through what happens when you plant MongoDB in your garden.*

---

## The Story

You have three Stones in your garden. An old ThinkPad, a Dell thin client, and a Raspberry Pi. They've been humming along for a few weeks now, and today you want to add MongoDB.

You open a terminal and type:

```bash
garden-rake plant mongodb on stone-amber-ridge
```

A brief pause. Then:

```
Planted mongodb on stone-amber-ridge (192.168.1.42:27017)
```

That's it. MongoDB is running.

---

Three days later, you're on your laptop—not one of the Stones, just a regular machine with Rake installed. You can't remember which Stone has MongoDB.

```bash
garden-rake find mongodb
```

```
mongodb available on stone-amber-ridge (192.168.1.42:27017)
  Health: healthy
  Version: 7.0.5
  Running for: 3 days, 2 hours
```

You update your application's config:

```
MONGODB_URI=zen-garden:mongodb/mydb
```

The app starts. It connects. You never typed an IP address.

---

A week passes. You run your morning check:

```bash
garden-rake nourish
```

```
Summary: 1 available update

  stone-amber-ridge
    AVAILABLE:
      • mongodb 7.0.5 → 7.0.6
```

You decide to update:

```bash
garden-rake nourish mongodb
```

```
Nourishing mongodb on stone-amber-ridge...

  [1/3] Collecting harvest...
        Quiescing database
        Archiving volumes
        Resuming database
        Harvest stored (2.3 GB)

  [2/3] Applying update...
        Pulling mongo:7.0.6
        Recreating container

  [3/3] Verifying health...
        Health check passed

Nourished mongodb: 7.0.5 → 7.0.6
```

MongoDB was never down. Your application didn't notice.

---

Six months later. Stone-amber-ridge starts making clicking sounds. The hard drive is dying.

You have a seed bank—a USB drive you prepared months ago that's been quietly receiving snapshots every night. You unplug it from the dying Stone.

You grab an old laptop from the closet. Install Moss. Plug in the USB drive.

```bash
garden-rake restore from seed-bank
```

```
Found seed bank: seed-amber-brook
Last snapshot: 6 hours ago

Restoring garden state...
  • stone-amber-ridge identity
  • mongodb (7.0.6) with 2.3 GB data
  • redis (7.2.3) with 156 MB data

Restore complete. Starting offerings...

  mongodb: healthy
  redis: healthy

Garden restored. This machine is now stone-amber-ridge.
```

Your applications reconnect. They find MongoDB at a new IP address, but they don't care—they asked for `zen-garden:mongodb` and they got it.

The old laptop with the dying drive? You can recycle it now. The garden moved on.

---

## What Just Happened

Let's rewind to the beginning and look behind the curtain.

### The Discovery Dance

When you typed `garden-rake plant mongodb on stone-amber-ridge`, Rake didn't know where stone-amber-ridge was. It had a name, not an address.

So Rake called out to the network: a UDP broadcast to port 7184, asking "Who's out there?"

Every Stone heard this. But they didn't all respond at once—that would be chaos. Each Stone calculated a small delay based on a hash of its name. The Stone with the lowest hash answered first, sharing its complete topology cache: every Stone it knows about, their addresses, what they're running.

From this single response, Rake learned that stone-amber-ridge lives at 192.168.1.42. Now it could send the real request.

### The Planting

Rake sent an HTTP request to Moss on stone-amber-ridge:

```
POST /api/v1/offerings
{ "template": "mongodb" }
```

Moss looked up "mongodb" in its embedded manifest library—not just a Docker image name, but a complete specification:

```yaml
name: mongodb
image: mongo:7.0.5
ports: [27017]
volumes:
  - data:/data/db
health:
  test: ["mongosh", "--eval", "db.runCommand('ping')"]
  interval: 30s
ceremony:
  mode: quiesceable
  quiesce: ["mongosh", "--eval", "db.fsyncLock()"]
  resume: ["mongosh", "--eval", "db.fsyncUnlock()"]
```

Moss generated a Docker Compose fragment, merged it with existing services, and ran the container. It waited for health checks to pass—three successful pings to the MongoDB shell—before reporting success.

### The Announcement

The moment MongoDB came online, Moss broadcast an mDNS announcement:

```
_mongodb._koan-stone._tcp.local.
  TXT: offering=mongodb
  TXT: version=7.0.5
  TXT: health=healthy
  TXT: stone=stone-amber-ridge
```

This is the same protocol your phone uses to find AirPlay speakers. Every Stone in the garden heard this and updated their topology cache. Within seconds, the entire garden knew MongoDB existed and where to find it.

If you had Cricket (the audio Companion) connected, you heard a soft chime. The garden grew.

### The Later Discovery

Three days later, when you ran `garden-rake find mongodb` from your laptop, the same discovery dance happened. Rake broadcast, a Stone responded with topology, and Rake found MongoDB in the cached data.

Your application did this too. When it connected to `zen-garden:mongodb/mydb`, the Zen Garden client library performed discovery, found stone-amber-ridge, and translated your connection string:

```
zen-garden:mongodb/mydb  →  mongodb://192.168.1.42:27017/mydb
```

Your code never saw the IP. If MongoDB moves tomorrow, discovery returns the new address. The connection string never changes.

### The Ceremony

When you ran `garden-rake nourish mongodb`, you triggered a *ceremony*—a deliberate, multi-phase operation with safety guarantees.

**Phase 1: Collect**

Before touching anything, Moss created a harvest. The template declared `mode: quiesceable`, meaning MongoDB can be snapshotted without stopping:

1. Moss ran `db.fsyncLock()` — MongoDB flushed writes to disk and paused
2. Moss committed the container state and archived the data volume
3. Moss ran `db.fsyncUnlock()` — MongoDB resumed

The database never went down. The harvest—a complete backup—sat ready in case anything went wrong.

**Phase 2: Nourish**

Moss pulled the new image, stopped the container, recreated it with the new image but the same volumes, and started it.

**Phase 3: Water**

Moss ran health checks. MongoDB responded to pings. The ceremony completed.

If health checks had failed, Moss would have stopped the new container, restored from the harvest, and restarted the old version. You would have seen: "Nourishment failed, rolled back to 7.0.5." Data safe, service restored.

### The Seed Bank

That USB drive you prepared months ago? It became a seed bank when you ran:

```bash
garden-rake prepare seed-bank
```

Moss formatted it with a special structure:

```
.zen-garden/
├── manifest.yaml           # Identity and sync info
├── journal/                # Change log for sync
├── garden/
│   ├── offerings/          # Offering snapshots
│   │   └── mongodb/
│   │       └── 2024-01-15/
│   │           ├── manifest.yaml
│   │           └── data.archive.gz
│   └── stones/
│       └── stone-amber-ridge/
│           └── identity.yaml
```

Every night, the nurturing scheduler created snapshots and wrote them to the seed bank. Volume data, offering configurations, Stone identities—everything needed to rebuild.

When the hard drive started dying and you ran `restore from seed-bank`, Moss on the new laptop read this structure. It adopted the identity of stone-amber-ridge (so applications would find it at the same name), unpacked the offering archives, and started the containers.

The new laptop *became* stone-amber-ridge. The garden reconverged around it.

### The Bigger Pattern

The traditional model: machines are the unit. You backup *disks*, monitor *hosts*, configure *servers*.

Zen Garden inverts this. The offering is the unit. MongoDB doesn't live "on stone-amber-ridge"—it lives "in the garden" and is currently placed on stone-amber-ridge. When you backup, you backup offerings. When you restore, you restore capabilities.

This is why hardware replacement is simple. The machine is incidental. The offering—its data, its configuration, its identity—flows through the garden wherever it needs to go.

---

## Commands From This Journey

```bash
# Planting
garden-rake plant mongodb on stone-amber-ridge

# Finding
garden-rake find mongodb
garden-rake list offerings

# Updating
garden-rake nourish                    # Check what's available
garden-rake nourish mongodb            # Update specific offering

# Seed Banks
garden-rake prepare seed-bank          # Initialize a USB drive
garden-rake restore from seed-bank     # Rebuild from backup
```

---

*Zen Garden Documentation — Journeys*
