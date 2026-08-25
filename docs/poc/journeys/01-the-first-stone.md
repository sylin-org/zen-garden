# The First Stone

_You have an old laptop and twenty minutes._

---

## The Story

The laptop has been sitting in a drawer for three years. A 2014 ThinkPad with a cracked screen, 8GB of RAM, and a spinning hard drive that still works. Windows 10 runs slowly on it. Windows 11 won't install at all.

Today, it becomes useful again.

---

You plug a USB drive into your main computer and run the installer script:

```powershell
.\NewStone-linux-x64.ps1 -UsbDrive "E:" -StoneName "stone-01"
```

```
Downloading Debian 12 net-install...    [████████████████████] 100%
Creating preseed configuration...       Done
Injecting garden-moss package...        Done
Writing bootable image...               Done

USB ready. Boot your target machine from this drive.
```

You pull the USB drive. Walk over to the old laptop. Plug it in. Power on. Press F12 for the boot menu. Select the USB drive.

The screen fills with text.

---

For the next fifteen minutes, you watch Debian install itself. Partitioning. Packages. Network configuration. You don't touch anything—the preseed file answers every question automatically.

The laptop reboots.

More text scrolls past. Then the console clears and you see:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Zen Garden - First Boot

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Temporary Name:    stone-01
  Starting first-time setup...

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Name Generation

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Generating candidate...   stone-amber-ridge
  Checking for collisions...   None found
  ✓ Name accepted

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       System Configuration

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Setting hostname...          stone-amber-ridge
  Updating /etc/hosts...       Done
  Restarting mDNS service...   Done
  Testing resolution...        stone-amber-ridge.local → 192.168.1.42

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Setup Complete

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Stone Name:       stone-amber-ridge
  Management URL:   http://192.168.1.42:7185

  ✓ Stone is ready for use

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

The laptop has a name now. Not "stone-01"—that was just a placeholder. It generated its own identity: stone-amber-ridge.

---

You go back to your main computer. Open a terminal.

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Debian 12 • Up 2m 15s

   CPU:     Intel Core i5-4200U (4 cores)
   Memory:  7.7 GB (92% free)
   Disk:    223 GB HDD (98% free)

   OFFERINGS:
   (none)

   Companions:
   (none)
```

There it is. The old laptop, announcing itself to your network. You didn't configure anything. You didn't edit any files. It just... appeared.

---

Time to give it something to do.

```bash
garden-rake offer mongodb on stone-amber-ridge
```

```
Checking compatibility...

  stone-amber-ridge
    ✓ Architecture: x86_64
    ✓ Memory: 7.7 GB (requires 512 MB)
    ✓ Disk: 218 GB free (requires 1 GB)

Placing mongodb on stone-amber-ridge...

  Pulling mongo:7.0.5...           [████████████████████] 100%
  Creating container...            Done
  Starting service...              Done
  Waiting for health check...      Passed

✓ Planted mongodb on stone-amber-ridge (192.168.1.42:27017)
```

---

You run observe again:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Debian 12 • Up 8m 32s

   OFFERINGS:
   └─ mongodb     Running   mongo:7.0.5   Healthy   27017
```

MongoDB is running on the laptop that couldn't run Windows 11.

You update your application's environment file:

```
MONGODB_URI=zen-garden:mongodb/myapp
```

Start your app. It connects. No IP address in your configuration. Just "give me MongoDB" and the garden provides.

---

The laptop sits on your desk now. The cracked screen doesn't matter—you never look at it. The fan spins occasionally. A small green LED blinks. Somewhere inside, a database is answering queries.

It's not an old laptop anymore.

It's a Stone.

---

## What Just Happened

Let's rewind and look at the machinery.

### The USB Installer

When you ran `NewStone-linux-x64.ps1`, it didn't just copy an image to the USB drive. It created a customized installation environment:

1. Downloaded the Debian 12 net-install ISO
2. Injected a "preseed" file—a script that answers every installation question automatically
3. Added the garden-moss `.deb` package to the installation media
4. Configured systemd to start Moss on boot

The preseed file is the key. Without it, you'd sit through twenty prompts: timezone, keyboard layout, disk partitioning, user accounts. The preseed answers all of them, turning a manual installation into an automated one.

### The First Boot Sequence

When the laptop booted after installation, systemd started the garden-moss service. Moss detected it was running with the temporary name "stone-01" and triggered first-boot initialization:

```
1. Generate unique name
   ├── Combine adjective + noun from word lists
   ├── Check network for collisions (mDNS browse)
   └── Accept if unique, retry if collision found

2. Configure system identity
   ├── Write new name to /etc/hostname
   ├── Update /etc/hosts (localhost mapping)
   ├── Restart Avahi (mDNS daemon)
   └── Verify resolution works

3. Update Moss configuration
   └── Write stone_name to config.toml

4. Display summary on TTY1 console
```

The name generation uses word lists—adjectives like "amber," "coral," "golden" and nouns like "ridge," "valley," "creek." The combination creates memorable, unique identifiers. If another Stone already has "stone-amber-ridge," the process generates a new candidate and checks again.

### Hardware Detection

While first-boot was running, a background task began detecting hardware capabilities:

```rust
HardwareCapabilities {
    stone_name: "stone-amber-ridge",
    stone_id: "01956a3e-7c00-7000-8000-abc123def456",

    cpu: CpuCapabilities {
        arch: "x86_64",
        cores: 4,
        features: ["sse4.2", "avx"],  // Important for some databases
    },

    memory: MemoryCapabilities {
        total_bytes: 8262144000,
        available_bytes: 7604051968,
    },

    inventory: HardwareInventory {
        storage_type: "HDD",
        gpu_devices: [],  // No discrete GPU
    },

    detection_status: DetectionStatus::Complete,
}
```

This information gets cached to disk. On subsequent boots, Moss loads the cache instead of re-detecting—startup becomes faster.

The CPU features matter. MongoDB 5.0+ requires AVX instructions. If this laptop had an older CPU without AVX, Moss would know to offer MongoDB 4.4 instead, or warn you about the incompatibility.

### The mDNS Announcement

The moment Moss finished initializing, it broadcast an announcement to the local network:

```
_koan-stone._tcp.local.
  TXT: stone_name=stone-amber-ridge
  TXT: stone_id=01956a3e-7c00-7000-8000-abc123def456
  TXT: version=0.2.1
  TXT: http_port=7185
```

This is mDNS—multicast DNS, the same protocol that lets your phone find AirPlay speakers. Every device on the local subnet received this announcement. If you had other Stones running, they would have updated their topology caches automatically.

When you ran `garden-rake observe` from your main computer, Rake didn't scan IP addresses or read configuration files. It broadcast a query: "Who's out there?" Stone-amber-ridge answered.

### The Offering Placement

When you ran `garden-rake offer mongodb`, several things happened:

**1. Compatibility Check**

Rake asked Moss for the Stone's capabilities, then compared them against MongoDB's requirements:

```yaml
# mongodb template (embedded in Moss)
name: mongodb
image: mongo:7.0.5
requirements:
  memory_mb: 512
  disk_gb: 1
  cpu_features: [] # 7.0.5 doesn't require AVX
```

All requirements passed. If they hadn't, Rake would have shown which constraints failed.

**2. Container Creation**

Moss generated a Docker Compose fragment:

```yaml
services:
  mongodb:
    image: mongo:7.0.5
    container_name: zg-mongodb
    ports:
      - "27017:27017"
    volumes:
      - mongodb-data:/data/db
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "mongosh", "--eval", "db.runCommand('ping')"]
      interval: 30s
      retries: 3

volumes:
  mongodb-data:
```

This got merged with any existing services and written to the compose file. Then Moss ran `docker compose up -d`.

**3. Health Verification**

Moss waited for the health check to pass—three successful pings to the MongoDB shell. Only after health checks passed did it update the offerings registry.

**4. Service Announcement**

A new mDNS announcement went out:

```
_mongodb._koan-stone._tcp.local.
  TXT: offering=mongodb
  TXT: version=7.0.5
  TXT: port=27017
  TXT: health=healthy
  TXT: stone=stone-amber-ridge
```

Now every device on the network knows: MongoDB is available at stone-amber-ridge:27017.

### The Connection String

When your application connected to `zen-garden:mongodb/myapp`, here's what the client library did:

1. Queried mDNS for services with `offering=mongodb`
2. Received the TXT record pointing to stone-amber-ridge:27017
3. Rewrote the connection string: `mongodb://192.168.1.42:27017/myapp`
4. Connected using the standard MongoDB driver

Your code never saw the IP address. If you replace the laptop tomorrow, the new Stone announces MongoDB, and your app reconnects to the new location. The connection string in your config file never changes.

### What Made This Possible

The laptop didn't become useful because of any single technology. It became useful because of how the pieces fit together:

- **Preseed automation** meant you didn't need Linux expertise to install
- **First-boot initialization** meant you didn't need to configure hostnames or networking
- **mDNS discovery** meant you didn't need to know IP addresses
- **Hardware detection** meant the system knows its own capabilities
- **Curated templates** meant you didn't need to write Docker configurations
- **Health checks** meant the system knows when services are actually ready

Each piece removes a decision you'd otherwise have to make. The cumulative effect: an old laptop becomes a database server in twenty minutes, with no expertise required.

---

## Commands From This Journey

```bash
# Create bootable USB (Windows PowerShell)
.\NewStone-linux-x64.ps1 -UsbDrive "E:" -StoneName "stone-01"

# Discover Stones on network
garden-rake observe

# Deploy a service
garden-rake offer mongodb on stone-amber-ridge

# Check Stone status
garden-rake status stone-amber-ridge
```

---

_Zen Garden Documentation — Journeys_
