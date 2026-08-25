# The Night the Drive Died

*You wake up to a clicking sound.*

---

## The Story

It's 2 AM. Something woke you up. A rhythmic clicking from the other room, like a broken clock trying to tick.

You know that sound. Every computer person knows that sound.

You get up and walk to the shelf where your Stones live. Stone-amber-ridge—the old ThinkPad—is making the noise. The hard drive is dying. Click. Click. Click.

---

Three months ago, you had a moment of paranoia.

You were looking at your garden—MongoDB on one Stone, Redis on another, your personal wiki, your password manager's database—and thought: "What if this all just... stopped?"

So you bought a USB drive. 64GB, nothing fancy. Plugged it into stone-amber-ridge and ran:

```bash
garden-rake prepare seed-bank
```

```
Preparing device: SANDISK_64GB (/dev/sdb1)

  Formatting as btrfs...           Done
  Creating .zen-garden structure... Done
  Registering seed bank...         Done

✓ Seed bank ready: seed-amber-brook
  Path: /var/lib/zen-garden/mounts/seed-amber-brook
  Capacity: 57.3 GB available
```

The garden named the seed bank automatically. Seed-amber-brook—because it feeds into the garden, like a brook feeds into a pond.

Then you enabled automatic backups:

```bash
garden-rake nurturing enable --daily
```

```
Nurturing enabled. Daily snapshots will be stored to seed-amber-brook.
```

And you forgot about it. For three months.

---

Now it's 2 AM and the drive is dying.

You unplug stone-amber-ridge. The clicking stops. The Stone goes dark.

You pull the USB drive from its port. The seed bank. Your backup.

---

The next morning, you find another old laptop in the closet. A 2012 MacBook someone gave you. You've never used it because macOS doesn't run well on it anymore.

You create a bootable USB installer. Boot the MacBook. Watch Debian install itself. First-boot runs:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Name Generation

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Generating candidate...      stone-silver-creek
  ✓ Name accepted

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Setup Complete

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Stone Name:       stone-silver-creek
  Management URL:   http://192.168.1.67:7185

  ✓ Stone is ready for use
```

A new Stone with a new name. But this isn't what you want. You want to restore stone-amber-ridge.

---

You plug in the seed bank USB drive. It auto-mounts. Then:

```bash
garden-rake restore from seed-bank
```

```
Found seed bank: seed-amber-brook
Last snapshot: 6 hours ago (02:15 AM)

Offerings available:
  • mongodb (7.0.6) - 2.3 GB data
  • wiki (latest) - 847 MB data
  • vaultwarden (1.30.1) - 12 MB data

Stone identity available:
  • stone-amber-ridge

Restore options:
  [1] Full restore (identity + all offerings)
  [2] Offerings only (keep current identity)
  [3] Select specific offerings
  [Q] Cancel

Choice: 1
```

You choose full restore.

```
Restoring stone identity...
  Adopting identity: stone-amber-ridge
  Updating hostname...
  Restarting mDNS...

Restoring offerings...
  [1/3] mongodb
        Extracting data archive... 2.3 GB
        Creating container...
        Starting service...
        Health check passed ✓

  [2/3] wiki
        Extracting data archive... 847 MB
        Creating container...
        Starting service...
        Health check passed ✓

  [3/3] vaultwarden
        Extracting data archive... 12 MB
        Creating container...
        Starting service...
        Health check passed ✓

✓ Restore complete

This machine is now stone-amber-ridge.
All offerings restored and healthy.
```

---

You check from your main computer:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.67)
   Moss 0.2.1 • Debian 12 • Up 5m

   OFFERINGS:
   ├─ mongodb       Running   Healthy   27017
   ├─ wiki          Running   Healthy   3000
   └─ vaultwarden   Running   Healthy   8080

●  stone-coral-reef (192.168.1.58)
   OFFERINGS:
   └─ redis         Running   Healthy   6379
```

Stone-amber-ridge is back. Different hardware—a MacBook instead of a ThinkPad—but the same identity, the same services, the same data.

Your password manager still works. Your wiki has all your notes. MongoDB has all your application data.

The garden healed.

---

Later, you test your applications:

```bash
# Your app still uses the same connection string
MONGODB_URI=zen-garden:mongodb/myapp

# It connects without changes
./start-app.sh
```

```
Connected to MongoDB at 192.168.1.67:27017
```

The IP address changed (the new machine got a different DHCP lease), but your application didn't care. It asked for `zen-garden:mongodb` and the garden answered. The new Stone announced the same services as the old one.

From your application's perspective, nothing happened. MongoDB was briefly unavailable during the night, then it came back. Business as usual.

---

## What Just Happened

### The Seed Bank Structure

When you prepared the seed bank three months ago, Moss created a specific directory structure:

```
/mnt/seed-amber-brook/.zen-garden/
├── manifest.yaml           # Seed bank identity
├── journal/                # Sync log (for multi-device sync)
└── garden/
    ├── index.yaml          # Backup inventory
    ├── offerings/
    │   ├── mongodb/
    │   │   └── 2026-01-15T02-15-00/
    │   │       ├── manifest.yaml
    │   │       └── data.archive.gz
    │   ├── wiki/
    │   │   └── 2026-01-15T02-15-00/
    │   │       ├── manifest.yaml
    │   │       └── data.archive.gz
    │   └── vaultwarden/
    │       └── 2026-01-15T02-15-00/
    │           ├── manifest.yaml
    │           └── data.archive.gz
    └── stones/
        └── stone-amber-ridge/
            └── identity.yaml
```

Each offering has a timestamped snapshot folder containing:
- **manifest.yaml**: Metadata (version, ports, volumes, health config)
- **data.archive.gz**: Compressed archive of the offering's data volumes

The stone identity file preserves:
- The stone's unique ID
- Its generated name
- Any custom configuration

### The Nightly Snapshots

When you enabled daily nurturing, a scheduler started running at 2 AM:

```
1. For each offering on the stone:
   a. Check if offering is stateful (has volumes)
   b. If stateful, create snapshot:
      - Quiesce if possible (db.fsyncLock() for MongoDB)
      - Archive volumes to temporary location
      - Resume if quiesced
      - Copy archive to seed bank
      - Update index.yaml
   c. Rotate old snapshots (keep last N)

2. Backup stone identity
```

The snapshot at 2:15 AM was the last one before the drive failed. Six hours of data were in RAM when the drive died, but everything up to 2:15 AM was safe on the USB drive.

### The Restore Process

When you ran `restore from seed-bank`, Moss on the new machine:

**1. Read the seed bank manifest**
```yaml
# .zen-garden/manifest.yaml
id: 019bf83e-7c00-7000-8000-abc123def456
name: seed-amber-brook
created_at: 2025-10-15T14:30:00Z
created_by:
  stone: stone-amber-ridge
```

**2. Scanned available backups**
```yaml
# garden/index.yaml
offerings:
  - name: mongodb
    latest_snapshot: 2026-01-15T02-15-00
    size_bytes: 2469606195
  - name: wiki
    latest_snapshot: 2026-01-15T02-15-00
    size_bytes: 888078336
  - name: vaultwarden
    latest_snapshot: 2026-01-15T02-15-00
    size_bytes: 12582912

stones:
  - name: stone-amber-ridge
    id: 019bf83e-ec4d-7371-98f0-fad4acb5938b
```

**3. Adopted the stone identity**
```bash
# Internal operations:
# 1. Change hostname to stone-amber-ridge
# 2. Update /etc/hosts
# 3. Restart Avahi for mDNS
# 4. Update Moss config with stone_id
```

**4. Restored each offering**
```rust
for offering in selected_offerings {
    // Read manifest
    let manifest = read_offering_manifest(&offering)?;

    // Extract volume data
    extract_archive(&offering.data_archive, &volume_path)?;

    // Create container with saved configuration
    docker.create_container(&manifest.container_config)?;

    // Start and verify health
    docker.start_container(&container_id)?;
    wait_for_health_check(&manifest.health_config)?;

    // Register in offerings registry
    registry.register(&offering)?;
}
```

**5. Announced to the network**
Once everything was healthy, the restored Stone broadcast its mDNS announcements—the same services, the same names, but at a new IP address.

### Identity Preservation

The key insight: the seed bank preserved the *identity* of stone-amber-ridge, not just its data.

When applications connect to `zen-garden:mongodb`, they're looking for a service announced by a Stone. The Stone's name is part of that announcement. If the new machine had kept its generated name (stone-silver-creek), discovery would have worked, but the garden's topology would be different.

By restoring the identity, the new machine *becomes* stone-amber-ridge. From the garden's perspective, the original Stone came back online after being offline for a few hours. That's exactly what happened—the hardware changed, but the Stone persisted.

### What Was Lost

Let's be honest about what the seed bank couldn't save:

- **6 hours of writes**: Anything written to MongoDB between 2:15 AM and when the drive died was lost. The applications kept running (Moss and Docker stayed in memory), but the data never made it to the seed bank.

- **In-flight state**: Any background jobs that were running, any temporary files, anything not in a volume—gone.

- **The old hardware**: The ThinkPad's drive is dead. You could try data recovery, but the seed bank made that unnecessary.

For a home lab, losing 6 hours of data is usually acceptable. For critical data, you'd want more frequent snapshots—hourly, or even continuous replication to a second Stone.

### The Alternative: No Seed Bank

Imagine this scenario without a seed bank:

1. Drive dies at 2 AM
2. You have no backup
3. You install a fresh Stone
4. You manually reinstall each service
5. You try to remember your configurations
6. Your data is gone

Or:

1. Drive dies at 2 AM
2. You pay for professional data recovery ($500-2000)
3. Maybe they recover the data, maybe they don't
4. It takes weeks
5. Your garden is down the entire time

The USB drive cost $15. The nightly snapshots ran automatically. The restore took 10 minutes.

That's the value proposition of a seed bank: cheap insurance against hardware failure.

---

## The Second Seed Bank

A week after the drive failure, you have another moment of paranoia.

The seed bank was plugged into the Stone that died. What if the power surge had killed the USB drive too? What if there had been a fire?

You buy another USB drive. Different brand, different capacity. You take it to work.

```bash
# At home
garden-rake prepare seed-bank KINGSTON_128GB as seed-backup-vault
```

```
✓ Seed bank ready: seed-backup-vault
```

Now you have two seed banks. They can sync with each other—when both are plugged in, changes propagate. But more importantly, you can keep one offsite.

Once a week, you swap the drives. Take the home one to work, bring the work one home. If your house burns down, you have a seed bank at the office. If the office floods, you have one at home.

Different physical locations. Same garden data. That's resilience.

---

## Commands From This Journey

```bash
# Prepare a seed bank
garden-rake prepare seed-bank
garden-rake prepare seed-bank SANDISK_64GB as my-backup

# Enable automatic backups
garden-rake nurturing enable --daily
garden-rake nurturing enable --hourly

# Check backup status
garden-rake nurturing status

# List available backups
garden-rake nurturing list mongodb

# Manually trigger a backup
garden-rake nurturing trigger mongodb
garden-rake nurturing trigger all

# Restore from seed bank
garden-rake restore from seed-bank

# Restore specific offering
garden-rake restore mongodb from seed-bank

# List seed banks
garden-rake show seed-banks

# Safely remove seed bank before unplugging
garden-rake release seed-bank seed-amber-brook
```

---

*Zen Garden Documentation — Journeys*
