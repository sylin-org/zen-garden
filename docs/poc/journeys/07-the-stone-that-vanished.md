# The Stone That Vanished

*Stone-coral-reef just... stopped responding.*

---

## The Story

### The Planned Shutdown

Friday afternoon. Stone-coral-reef needs a BIOS update that requires a reboot. You run:

```bash
garden-rake slumber stone-coral-reef
```

```
Shutting down stone-coral-reef...
  Sending goodbye to garden
  Stopping Moss daemon
  Initiating system shutdown
  ✓ Stone is offline
```

Your application, which uses Redis on stone-coral-reef, shows in its logs:

```
[16:45:12] Zen Garden: stone-coral-reef going offline
[16:45:12] Zen Garden: Redis now available on stone-amber-ridge
[16:45:12] Connected to Redis (failover: stone-coral-reef → stone-amber-ridge)
```

This is a balanced service—you deployed Redis on both Stones earlier this week for exactly this scenario. When stone-coral-reef announced its goodbye, the driver instantly found Redis on stone-amber-ridge and switched over.

Your application didn't even hiccup. It received the topology update through its driver event hook and reconnected before the old connection dropped.

---

### The Crash

Tuesday, 3 PM. You're deep in a coding session when your application logs start showing errors:

```
[15:02:34] ERROR: Redis connection refused
[15:02:35] ERROR: Redis connection refused
[15:02:36] Reconnecting to zen-garden:redis...
[15:02:37] ERROR: Service not found: redis
```

That's strange. Redis has been running for months without issues.

You check the garden:

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Up 45d

   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

○  stone-coral-reef (last seen 2m ago)
   Status: Offline
```

Stone-coral-reef is gone. The machine that runs Redis just... vanished.

---

You walk to the shelf where the Stones live. Stone-coral-reef—the Dell thin client—is dark. No power LED. No fan noise. Nothing.

You check the power strip. The switch got bumped. You flip it back on.

The thin client whirs to life. Boot sequence. Debian loading. Moss starting.

Back at your desk:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

●  stone-coral-reef (192.168.1.58)
   Moss 0.2.1 • Up 45s

   OFFERINGS:
   └─ redis       Running   Healthy   6379
```

Stone-coral-reef is back. Redis is running. Your application logs:

```
[15:05:12] Reconnecting to zen-garden:redis...
[15:05:12] Resolved zen-garden:redis → redis://192.168.1.58:6379
[15:05:12] Connected to Redis
```

Total outage: about 3 minutes. No manual intervention except flipping the power switch.

---

But what happened during those 3 minutes? Unlike the planned shutdown, there was no goodbye chirp—the power just vanished. Your application kept trying to connect to Redis. For the first 45 seconds, it got "connection refused"—the old IP address, but no response. Then it started getting "service not found."

Let's trace the timeline for an unclean shutdown:

```
15:02:30  Power strip switched off
          Stone-coral-reef loses power instantly
          Redis stops (no graceful shutdown)

15:02:31  Your application's Redis connection drops
          "Connection refused" - TCP can't connect

15:02:31  Stone-amber-ridge still thinks stone-coral-reef is online
          (Last chirp was 10 seconds ago, within 45s threshold)

15:03:15  Stone-amber-ridge maintenance task runs
          No chirp from stone-coral-reef in 45 seconds
          Marks stone-coral-reef as OFFLINE
          Removes redis from topology cache

15:03:16  Your application retries discovery
          "Service not found: redis" - no Stone offers it anymore

15:05:00  Power strip switched back on
          Stone-coral-reef boots

15:05:10  Moss starts, Redis container auto-starts

15:05:11  Stone-coral-reef sends first chirp
          "I'm here, I have redis"

15:05:11  Stone-amber-ridge receives chirp
          Updates topology: stone-coral-reef is ONLINE
          Redis is available again

15:05:12  Your application retries discovery
          Finds redis on stone-coral-reef
          Connects successfully
```

---

You're curious about what your application saw during the outage. You check its retry logic:

```javascript
// Your application's connection code
async function getRedisConnection() {
  const maxRetries = 10;
  const retryDelay = 5000; // 5 seconds

  for (let i = 0; i < maxRetries; i++) {
    try {
      const uri = await resolve('zen-garden:redis');
      return await redis.connect(uri);
    } catch (err) {
      console.log(`Retry ${i + 1}/${maxRetries}: ${err.message}`);
      await sleep(retryDelay);
    }
  }
  throw new Error('Could not connect to Redis after 10 retries');
}
```

The application kept retrying every 5 seconds. After about 3 minutes, a retry succeeded. If the outage had lasted longer than 50 seconds (10 retries × 5 seconds), the application would have given up.

For a cache like Redis, this is usually fine. Cache misses are handled gracefully. But for a primary database, you might want more aggressive retries or a longer timeout.

---

Later, you wonder: what if you had multiple Stones running Redis? Would the application have failed over automatically?

You set up an experiment. Deploy Redis on both Stones:

```bash
garden-rake offer redis on stone-amber-ridge
```

Now you have Redis on two Stones. You check discovery:

```bash
garden-rake find redis
```

```
Found 2 offerings matching 'redis':

  redis on stone-coral-reef (192.168.1.58:6379)
    Health: healthy
    Priority: 50

  redis on stone-amber-ridge (192.168.1.42:6379)
    Health: healthy
    Priority: 50
```

Two Redis instances. Same priority. When your application connects to `zen-garden:redis`, which one does it get?

```bash
garden-rake find redis --format uri
```

```
redis://192.168.1.58:6379
```

It got stone-coral-reef—the first one discovered. But if you kill stone-coral-reef:

```bash
# Simulate outage
ssh stone@stone-coral-reef "sudo systemctl stop garden-moss"
```

Wait 45 seconds for the topology to update, then:

```bash
garden-rake find redis --format uri
```

```
redis://192.168.1.42:6379
```

Discovery now returns stone-amber-ridge—the only healthy one. Your application, on its next reconnect, would automatically find the surviving instance.

This isn't true clustering or replication—the two Redis instances don't share data. But for caches, that's often fine. For databases that need replication, you'd set up actual database clustering. The garden just makes both instances discoverable.

---

## What Just Happened

### Two Paths to Offline

A Stone can go offline in two ways, and they're handled very differently:

**Graceful shutdown (goodbye chirp):**
```
Stone initiates shutdown
  │
  └─► Broadcasts STONE_GOODBYE to garden
        │
        └─► All other Stones instantly update topology
              │
              └─► Drivers receive event notification
                    │
                    └─► Apps failover immediately (0ms detection)
```

**Crash/power loss (no goodbye):**
```
Stone dies suddenly
  │
  └─► No goodbye sent (can't)
        │
        └─► Other Stones wait for missed chirps
              │
              └─► After 45 seconds, marked offline
                    │
                    └─► Apps on next request discover service gone
```

### The Goodbye Chirp

When you run `garden-rake slumber` or the Stone shuts down gracefully, Moss broadcasts a goodbye:

```json
{
  "msg_id": "01936e8b-1234-7def...",
  "type": "stone_goodbye",
  "data": {
    "stone_id": "01936e8a-7b2c-7def...",
    "stone_name": "stone-coral-reef",
    "reason": "shutdown"
  }
}
```

Every Stone in the garden receives this. They immediately:
1. Mark stone-coral-reef as **offline** in their topology cache
2. Retain its MAC address (for Wake-on-LAN later)
3. Remove its services from discovery results

This happens in milliseconds. No waiting for timeouts.

### Event-Driven Failover

Smart drivers—like the Koan Framework's ZenGardenClient—don't just cache service locations. They listen for topology changes:

```csharp
// Driver subscribes to chirp events
zenClient.OnTopologyChanged += (sender, e) =>
{
    if (e.Stone == currentRedisStone && e.Status == "offline")
    {
        // Immediately find Redis on another Stone
        var newRedis = await zenClient.FindServiceAsync("redis");
        RedisConnection = newRedis.ConnectionString;
    }
};
```

For **balanced services** like Redis (where multiple Stones might offer the same service), failover is instantaneous. The driver hears the goodbye, finds Redis on another Stone, and reconnects—all before the application's next Redis call.

Your application doesn't even notice. No errors. No retries. Just smooth operation.

For **singleton services** (only one Stone offers it), graceful shutdown still triggers immediate detection. The app knows the service is gone right away, rather than discovering it 45 seconds later when connections fail.

### The Crash Detection (No Goodbye)

When stone-coral-reef lost power, it didn't send a goodbye message. It just stopped.

Every Stone expects to hear chirps from its neighbors every 30 seconds. When a Stone misses chirps, the topology maintenance task notices:

```
Timeline of chirps:

15:02:00  stone-coral-reef chirps (normal)
15:02:30  stone-coral-reef loses power (no chirp sent)
15:02:30  stone-amber-ridge expects next chirp around 15:03:00

15:03:00  No chirp received
          Still within tolerance (one missed chirp is normal)

15:03:15  Maintenance task runs (every 30 seconds)
          Checks: when did I last hear from stone-coral-reef?
          Answer: 75 seconds ago (15:02:00)
          Threshold: 45 seconds
          Action: Mark stone-coral-reef as OFFLINE
```

The 45-second threshold is deliberate:
- **Too short** (10s): Network hiccups cause false alarms
- **Too long** (5min): Real outages go unnoticed
- **45 seconds** (1.5 chirp cycles): Tolerates one missed chirp, catches real failures

### The Topology Update

When stone-coral-reef was marked offline, the topology cache updated:

```rust
// Before
TopologyCache {
    "stone-coral-reef": TopologyEntry {
        status: Online,
        offerings: ["redis"],
        last_seen: "15:02:00",
    }
}

// After maintenance task
TopologyCache {
    "stone-coral-reef": TopologyEntry {
        status: Offline,      // Changed
        offerings: ["redis"], // Still remembered
        last_seen: "15:02:00",
    }
}
```

The entry isn't deleted immediately—it's marked offline. This preserves information:
- The Stone might come back soon
- We remember what services it had
- Wake-on-LAN needs the MAC address (stored in the entry)

After 24 hours offline, entries get evicted to prevent unbounded cache growth.

### Discovery During Outage

When your application tried `zen-garden:redis` during the outage:

```
1. Application calls resolve("zen-garden:redis")

2. Client library runs discovery cascade:
   - Localhost cache? No (app isn't on a Stone)
   - UDP broadcast → stone-amber-ridge responds with topology

3. Search topology for "redis":
   - stone-coral-reef has redis, but status = Offline
   - No other Stones have redis
   - Result: no healthy offerings found

4. Client library throws "Service not found: redis"
```

The key point: discovery filters out offline Stones. Even though stone-coral-reef is still in the cache, its offline status means its services aren't returned.

### The Recovery

When stone-coral-reef came back online:

```
15:05:10  Moss starts
          - Loads configuration
          - Connects to Docker
          - Finds redis container (still exists, auto-starts)
          - Waits for health check

15:05:11  Redis health check passes
          Moss sends first chirp:
          {
            "stone_name": "stone-coral-reef",
            "status": "online",
            "offerings": ["redis"],
            "endpoint": "192.168.1.58:7185"
          }

15:05:11  stone-amber-ridge receives chirp
          Updates topology:
          - stone-coral-reef: Offline → Online
          - redis is available again
```

No special "I'm back" protocol. Just a normal chirp. The garden treats recovery the same as initial discovery.

### What Applications Should Do

Your application handled the outage correctly by retrying. Here's the pattern:

```javascript
// Good: Retry with backoff
async function connectWithRetry(service, maxRetries = 10) {
  for (let i = 0; i < maxRetries; i++) {
    try {
      const uri = await resolve(`zen-garden:${service}`);
      return await connect(uri);
    } catch (err) {
      const delay = Math.min(1000 * Math.pow(2, i), 30000); // Exponential backoff, max 30s
      await sleep(delay);
    }
  }
  throw new Error(`Failed to connect to ${service}`);
}

// Good: Handle reconnection on disconnect
connection.on('disconnect', async () => {
  connection = await connectWithRetry('redis');
});
```

The garden provides discovery, but applications must handle:
- **Reconnection** when connections drop
- **Retries** when services are temporarily unavailable
- **Graceful degradation** when services are down

For a cache, graceful degradation might mean falling back to the database. For a primary database, it might mean showing an error page. The garden can't decide that for you.

---

## The 45-Second Window (Crashes Only)

When a Stone shuts down gracefully, there's no detection delay—the goodbye chirp triggers instant topology updates.

But when a Stone crashes or loses power unexpectedly, there's a window—up to 45 seconds—where the garden thinks it's still alive. During this window:

- Discovery returns the dead Stone's services
- Applications try to connect and fail
- Connections time out at the TCP level

This is a trade-off. You could reduce the threshold to 15 seconds and detect failures faster. But you'd also get more false alarms from network congestion or brief hiccups.

For most home labs, 45 seconds for crashes is fine. Your application's retry logic handles the gap. If you need faster failover, consider:

1. **Multiple instances** of critical services (event-driven drivers failover instantly)
2. **UPS with shutdown scripts** (converts crashes into graceful shutdowns)
3. **Shorter connection timeouts** in your application (fail fast, retry fast)

The garden is eventually consistent for crashes, instantly consistent for planned shutdowns. This prioritizes stability over false alarms.

---

## Commands From This Journey

```bash
# Check garden status
garden-rake observe

# See detailed topology
garden-rake status stone-amber-ridge --topology

# Find a service
garden-rake find redis

# Find all instances of a service
garden-rake find redis --all

# Force fresh discovery (skip cache)
garden-rake find redis --fresh

# Watch events in real-time
garden-rake watch stone-amber-ridge
```

---

*Zen Garden Documentation — Journeys*
