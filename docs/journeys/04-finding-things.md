# Finding Things

*You can't remember which Stone has Redis.*

---

## The Story

You're setting up a new application. It needs Redis. You know Redis is running somewhere in your garden—you deployed it weeks ago—but you can't remember which Stone.

You could SSH into each Stone and check. You could look through your notes. Or:

```bash
garden-rake find redis
```

```
redis available on stone-coral-reef (192.168.1.58:6379)
  Health: healthy
  Version: 7.2.3
  Running for: 23 days
```

There it is. Stone-coral-reef. You didn't have to remember.

---

Your application needs a connection string. The traditional way:

```
REDIS_URL=redis://192.168.1.58:6379
```

But what happens when stone-coral-reef dies and you move Redis to a different Stone? You'd have to update every application that connects to it. Find every config file. Restart every service.

Instead, you write:

```
REDIS_URL=zen-garden:redis
```

You start your application:

```bash
./start-app.sh
```

```
Connecting to Redis...
Resolved zen-garden:redis → redis://192.168.1.58:6379
Connected.
```

The application found Redis without knowing the IP address.

---

A month later, stone-coral-reef starts having problems. You decide to move Redis to stone-amber-ridge:

```bash
garden-rake vacate redis from stone-coral-reef to stone-amber-ridge
```

```
Moving redis...
  Creating snapshot on stone-coral-reef...
  Transferring to stone-amber-ridge...
  Starting on stone-amber-ridge...
  Health check passed.
  Stopping on stone-coral-reef...

✓ redis now running on stone-amber-ridge (192.168.1.42:6379)
```

Your application was running during this move. What happened to it?

```
[14:32:01] Connection to Redis lost
[14:32:01] Reconnecting...
[14:32:02] Resolved zen-garden:redis → redis://192.168.1.42:6379
[14:32:02] Connected.
```

The application lost connection for one second. It reconnected automatically—to a different IP address. You didn't touch any configuration files.

---

You have another application that needs a database, but you're not sure what kind. You just know it needs to store documents.

```bash
garden-rake find c:document-database
```

```
Found 2 offerings matching category 'document-database':

  mongodb on stone-amber-ridge (192.168.1.42:27017)
    Health: healthy
    Categories: database, document-database

  couchdb on stone-coral-reef (192.168.1.58:5984)
    Health: healthy
    Categories: database, document-database
```

You can search by category, not just by name. The `c:` prefix means "category."

---

Your colleague is working on the same garden from their laptop. They're on Windows—no mDNS support built in. They run:

```bash
garden-rake find mongodb
```

```
mongodb available on stone-amber-ridge (192.168.1.42:27017)
```

It works. They didn't install any special software. The garden found itself.

---

## What Just Happened

### The Discovery Cascade

When you ran `garden-rake find redis`, Rake didn't scan the network or query a database. It followed a cascade—trying methods in order until one worked:

```
┌─────────────────────────────────────────────────────────┐
│  DISCOVERY CASCADE                                       │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  1. LOCALHOST CACHE                                     │
│     Is there a Moss daemon on this machine?             │
│     └─ Yes: Query its topology cache (< 1ms)            │
│     └─ No: Continue to step 2                           │
│                                                         │
│  2. UDP DISCOVERY                                       │
│     Send multicast to 239.255.42.99:7184                │
│     Also broadcast to 255.255.255.255:7184 (Windows)    │
│     └─ Response received: Use topology (< 100ms)        │
│     └─ No response: Continue to step 3                  │
│                                                         │
│  3. mDNS BROWSE                                         │
│     Browse for _koan-stone._tcp.local                   │
│     └─ Services found: Build topology (< 50ms)          │
│     └─ Nothing found: Continue to step 4                │
│                                                         │
│  4. LANTERN QUERY                                       │
│     Query configured Lantern registry                   │
│     └─ Response: Use topology (< 200ms)                 │
│     └─ No Lantern configured: Fail                      │
│                                                         │
│  5. MANUAL OVERRIDE                                     │
│     garden-rake find redis --at stone-amber-ridge       │
│     └─ Query specific Stone directly                    │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

Most of the time, discovery stops at step 1 or 2. If you're running Rake on a Stone, localhost cache answers instantly. If you're on a laptop, UDP broadcast gets a response in milliseconds.

### The Topology Cache

Every Stone maintains a topology cache—a map of the entire garden:

```rust
TopologyCache {
    stones: {
        "stone-amber-ridge": {
            endpoint: "192.168.1.42:7185",
            offerings: ["mongodb", "redis"],
            last_seen: "2026-01-15T10:30:00Z",
            status: Online,
        },
        "stone-coral-reef": {
            endpoint: "192.168.1.58:7185",
            offerings: ["postgres", "nginx"],
            last_seen: "2026-01-15T10:30:05Z",
            status: Online,
        },
    }
}
```

This cache updates every time a Stone hears a chirp (every 30 seconds). When you query any Stone's topology, you get the entire garden's state.

The cascade's first step—localhost cache—works because if you're running Rake on a Stone, that Stone already knows everything. No network round-trip needed.

### UDP Discovery (Windows-Compatible)

Step 2 uses UDP broadcast, which works everywhere—including Windows without mDNS support.

```
Rake                          All Stones
  │                               │
  │─── Discovery Request ────────►│ (multicast 239.255.42.99:7184)
  │    "Who's out there?"         │
  │                               │
  │                          Each Stone calculates delay:
  │                          delay = hash(stone_id + request_id)[0] * 10ms
  │                               │
  │◄── First Response ────────────│ (from Stone with lowest hash)
  │    Full topology cache        │
  │                               │
  │    (Other Stones would respond, but Rake already has answer)
```

The hash-based delay prevents response storms. Only the "fastest" Stone (lowest hash) actually responds. One request, one response, full topology.

### Connection String Resolution

When your application connects to `zen-garden:redis`, here's what happens inside the client library:

```
1. Parse connection string
   Input: "zen-garden:redis"
   Service: "redis"
   Database: (none)

2. Discover service
   Run discovery cascade
   Find: redis on stone-coral-reef:6379

3. Build native connection string
   Output: "redis://192.168.1.58:6379"

4. Connect using standard driver
   Application connects normally
```

The magic is in step 3. The `zen-garden:` prefix triggers discovery. The result is a native connection string that any standard driver understands.

### Category Search

The `c:document-database` syntax searches by category instead of name:

```bash
garden-rake find c:document-database
```

Each offering template declares its categories:

```yaml
# mongodb template
name: mongodb
categories:
  - database
  - document-database

# couchdb template
name: couchdb
categories:
  - database
  - document-database

# postgres template
name: postgres
categories:
  - database
  - relational-database
```

When you search by category, the garden returns all offerings that match. This is useful when you care about *capability* ("I need a document database") rather than *specific software* ("I need MongoDB").

### The Reconnection

When Redis moved from stone-coral-reef to stone-amber-ridge, your application's connection broke. Here's what happened:

```
1. Connection lost
   TCP connection to 192.168.1.58:6379 fails

2. Application reconnects
   Calls connect("zen-garden:redis") again

3. Discovery runs
   Finds redis now on stone-amber-ridge:6379

4. New connection established
   TCP connection to 192.168.1.42:6379 succeeds
```

The key insight: your application code didn't change. It still connects to `zen-garden:redis`. Discovery returned a different IP address, and the standard Redis driver connected to it.

This is the decoupling that makes hardware replacement simple. The connection string points to a *service*, not a *machine*. When the machine changes, discovery returns the new location.

### Why Windows Works

Your Windows colleague didn't need special software because of step 2 in the cascade: UDP broadcast.

Windows has notoriously unreliable mDNS support. The `.local` domain doesn't resolve consistently. Bonjour helps but isn't always installed.

UDP broadcast, however, works everywhere. It's basic networking—send a packet to 255.255.255.255, everyone on the subnet receives it. No special services needed.

The garden detects the platform and chooses the right discovery method automatically. On Linux/macOS, mDNS is preferred (more elegant). On Windows, UDP broadcast is the primary path.

---

## The Find Command

The `find` command has several modes:

```bash
# Find by name
garden-rake find redis
garden-rake find mongodb

# Find by category
garden-rake find c:database
garden-rake find c:document-database

# Find with tags
garden-rake find c:database t:production

# Output formats
garden-rake find redis --format json
garden-rake find redis --format uri        # Outputs: redis://stone-coral-reef.local:6379
garden-rake find redis --format uri-ip     # Outputs: redis://192.168.1.58:6379

# Force fresh discovery (skip cache)
garden-rake find redis --fresh

# Wishful mode (auto-deploy if not found)
garden-rake find redis --ensure
```

The `--ensure` flag is interesting: if Redis isn't found, Rake will ask if you want to deploy it. "I needed Redis, and the garden provided."

---

## Connection Strings in Practice

Here's how different applications use Zen Garden connection strings:

**Environment variables:**
```bash
MONGODB_URI=zen-garden:mongodb/myapp
REDIS_URL=zen-garden:redis
POSTGRES_DSN=zen-garden:postgresql/production
```

**Docker Compose:**
```yaml
services:
  myapp:
    environment:
      - DATABASE_URL=zen-garden:postgresql/myapp
```

**Application config:**
```json
{
  "database": {
    "url": "zen-garden:mongodb/users"
  },
  "cache": {
    "url": "zen-garden:redis"
  }
}
```

The pattern is always the same: `zen-garden:<service>[/<database>]`

No IP addresses. No hostnames. Just "give me this service."

---

## When Discovery Fails

What if no Stone has the service you're looking for?

```bash
garden-rake find elasticsearch
```

```
No offerings found matching 'elasticsearch'

Available alternatives:
  • opensearch on stone-amber-ridge (compatible with Elasticsearch clients)

Or deploy elasticsearch:
  garden-rake offer elasticsearch --anywhere
```

The garden suggests alternatives and offers to deploy what you need.

What if the network is completely unreachable?

```bash
garden-rake find redis
```

```
Discovery failed: No Stones found

Troubleshooting:
  • Check network connectivity
  • Verify at least one Stone is running
  • Try direct access: garden-rake find redis --at stone-amber-ridge
```

The `--at` flag bypasses discovery entirely. If you know a Stone's address, you can always reach it directly.

---

## Commands From This Journey

```bash
# Find by name
garden-rake find redis
garden-rake find mongodb

# Find by category
garden-rake find c:database
garden-rake find c:document-database

# Output as connection string
garden-rake find redis --format uri

# Skip cache, fresh discovery
garden-rake find redis --fresh

# Auto-deploy if not found
garden-rake find redis --ensure

# Direct query (bypass discovery)
garden-rake find redis --at stone-amber-ridge

# List all offerings in garden
garden-rake observe
```

---

*Zen Garden Documentation — Journeys*
