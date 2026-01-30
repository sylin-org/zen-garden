# When Stones Meet

*You plug in the second machine.*

---

## The Story

Stone-amber-ridge has been running for a week. MongoDB hums along. Your application connects without complaint. One Stone feels like a curiosity—a project you got working.

Today you add a second Stone. That's when it becomes a garden.

---

You find a Dell Wyse thin client at the office e-waste pile. It's the size of a paperback book. 2GB of RAM, a 16GB flash drive, no moving parts. Someone was about to throw it away.

You take it home. Create another USB installer. Boot it up. Watch the same installation sequence. First-boot runs:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                       Name Generation

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Generating candidate...      stone-coral-reef
  Checking for collisions...   Found: stone-amber-ridge
  No conflict.
  ✓ Name accepted
```

The new Stone checked the network before accepting its name. It saw stone-amber-ridge out there. It made sure not to pick the same name.

---

From your laptop, you run observe:

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Debian 12 • Up 7d 4h

   OFFERINGS:
   └─ mongodb     Running   mongo:7.0.5   Healthy   27017

●  stone-coral-reef (192.168.1.58)
   Moss 0.2.1 • Debian 12 • Up 2m 45s

   OFFERINGS:
   (none)
```

Two Stones. They found each other without you doing anything.

---

You decide to add Redis to the new Stone:

```bash
garden-rake offer redis on stone-coral-reef
```

```
✓ Planted redis on stone-coral-reef (192.168.1.58:6379)
```

Run observe again:

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

●  stone-coral-reef (192.168.1.58)
   OFFERINGS:
   └─ redis       Running   Healthy   6379
```

Your application can now find both services:

```
MONGODB_URI=zen-garden:mongodb/myapp
REDIS_URL=zen-garden:redis
```

Two connection strings. Two Stones. No IP addresses.

---

You're curious. What does stone-amber-ridge know about its neighbor?

```bash
garden-rake status stone-amber-ridge --topology
```

```
Known Stones:
  stone-amber-ridge (self)    192.168.1.42    Online     7d 4h
  stone-coral-reef            192.168.1.58    Online     8m 12s
    └─ redis (6379)
```

Stone-amber-ridge knows about stone-coral-reef. It knows what services are running there. It learned this automatically.

You check the other direction:

```bash
garden-rake status stone-coral-reef --topology
```

```
Known Stones:
  stone-coral-reef (self)     192.168.1.58    Online     8m 30s
  stone-amber-ridge           192.168.1.42    Online     7d 4h
    └─ mongodb (27017)
```

They know each other. They know each other's services. No configuration file connects them. No central registry coordinates them. They just... talk.

---

That night, you unplug stone-coral-reef to move it to a different shelf. You forget to plug it back in.

The next morning:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

○  stone-coral-reef (last seen 8h ago)
   Status: Offline
```

Stone-amber-ridge noticed. It marked stone-coral-reef as offline. Not immediately—it waited about 45 seconds first, in case it was just a brief network glitch. After that, it updated its topology: "I haven't heard from this Stone in a while."

You plug it back in. Boot it up. Within thirty seconds:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

●  stone-coral-reef (192.168.1.58)
   OFFERINGS:
   └─ redis       Running   Healthy   6379
```

Back online. No intervention. The Stones found each other again.

---

## What Just Happened

### The Chirp

Every Stone in the garden sends a heartbeat every 30 seconds. It's called a "chirp"—a small UDP packet broadcast to the local network.

```
Multicast: 239.255.42.99:7184

Payload:
{
  "stone_id": "019bf83e-ec4d-7371-98f0-abc123",
  "stone_name": "stone-coral-reef",
  "http_endpoint": "192.168.1.58:7185",
  "version": "0.2.1",
  "offerings": ["redis"],
  "timestamp": "2026-01-15T10:30:00Z"
}
```

Every Stone is both sender and receiver. Stone-amber-ridge sends its chirp, stone-coral-reef hears it. Stone-coral-reef sends its chirp, stone-amber-ridge hears it. No central coordinator. Just voices in the dark, calling out "I'm here."

The chirp uses multicast—a special network address that all listening devices receive simultaneously. It's the same mechanism that lets your TV find Chromecast devices, or your laptop find network printers.

### The Topology Cache

When a Stone hears a chirp, it updates its topology cache:

```rust
TopologyCache {
    entries: {
        "019bf83e-ec4d-7371-98f0-abc123": TopologyEntry {
            stone_id: "019bf83e-ec4d-7371-98f0-abc123",
            stone_name: "stone-coral-reef",
            endpoint: "192.168.1.58:7185",
            offerings: ["redis"],
            last_seen: "2026-01-15T10:30:00Z",
            status: Online,
        },
        // ... other stones
    }
}
```

The cache is in-memory—fast to query, constantly updated. When you run `garden-rake observe`, Rake asks any Stone for its topology cache and gets a complete picture of the garden instantly.

### The Discovery Cascade

When you ran `garden-rake observe` from your laptop, your laptop isn't a Stone—it doesn't have Moss running, doesn't have a topology cache. So how did it discover the garden?

Rake used a discovery cascade:

**Step 1: Check localhost**
Is there a Moss daemon on this machine? No. Continue.

**Step 2: UDP broadcast**
Send a discovery request to the network. The request goes to `239.255.42.99:7184` (multicast) and also to `255.255.255.255:7184` (broadcast fallback for Windows).

**Step 3: Wait for response**
Every Stone that hears the request could respond. But they don't all respond at once—that would flood the network. Instead, each Stone calculates a delay:

```rust
delay_ms = blake3_hash(stone_id + request_id)[0] * 10
```

The Stone with the lowest hash responds first. By the time other Stones would respond, Rake already has an answer and stops listening. One request, one response—but that response contains the entire topology.

**Step 4: Use the topology**
Rake now has the same information every Stone has. It shows you the garden.

### Collision Detection

When stone-coral-reef first booted, it generated a name candidate: "stone-coral-reef". But before accepting it, Moss broadcast a discovery request to check for collisions.

```
1. Generate candidate: "stone-coral-reef"
2. Send discovery request
3. Receive topology (contains "stone-amber-ridge")
4. Check: is "stone-coral-reef" in the topology? No.
5. Accept name
```

If the name had been taken, Moss would generate another candidate and try again. This prevents the awkward situation of two Stones with the same name.

### The Offline Threshold

When you unplugged stone-coral-reef, it stopped chirping. Stone-amber-ridge didn't panic immediately—network hiccups happen. It uses a 45-second threshold:

| Time | What Happens |
|------|--------------|
| T+0s | Stone stops chirping |
| T+30s | Missed one chirp cycle. Still marked "online" |
| T+45s | Maintenance task runs. 1.5 cycles missed. Mark "offline" |

The 45-second threshold is deliberate: long enough to ignore brief network glitches, short enough to detect actual failures quickly.

After 24 hours offline, a Stone gets evicted from the cache entirely. But its identity isn't lost—if it comes back with the same stone_id, it's recognized as the same Stone.

### What the Stones Don't Do

It's worth noting what *doesn't* happen:

- **No central registry** (by default). Stones find each other directly via multicast.
- **No leader election** (for topology). Every Stone maintains its own view.
- **No synchronization protocol**. Each Stone has its own cache, updated by what it hears.
- **No persistent storage of topology**. If a Stone reboots, it rebuilds from chirps.

This simplicity is intentional. For a garden of 3-10 Stones on a single subnet, direct peer-to-peer discovery is simpler and more reliable than any coordinator.

### When Simple Isn't Enough

What if your Stones are on different subnets? Multicast doesn't cross routers. You can't hear a chirp from the other side of your house if your network has multiple VLANs.

That's when you light a Lantern—a registry service that Stones register with and clients query. But that's a different journey. For most home gardens, chirps are enough.

---

## The Moment It Clicked

Here's the thing about two Stones: one Stone is an experiment. Two Stones is a pattern.

With one Stone, you're running a service on old hardware. Interesting, but not transformative. You could do that with just Docker.

With two Stones, you have a system that *knows about itself*. Stone-amber-ridge knows what stone-coral-reef offers. Your applications can find either service without knowing which machine it's on. You can move services between Stones and nothing breaks.

That's the moment the garden becomes real. Not when you install the first Stone, but when the second one appears in the topology and they start talking to each other.

---

## Commands From This Journey

```bash
# See the garden
garden-rake observe

# Check a Stone's view of the topology
garden-rake status stone-amber-ridge --topology

# Deploy to a specific Stone
garden-rake offer redis on stone-coral-reef

# Watch discovery happen in real-time
garden-rake observe --watch
```

---

*Zen Garden Documentation — Journeys*
