# Crossing the Subnet

*Your Stones can't see each other. Time to light a Lantern.*

---

## The Story

Your garden has grown. Three Stones on your main network, running databases and services. But now you have a new project: a home automation Stone on your IoT VLAN.

You set up the Stone on the IoT network (192.168.10.x) and try to check on your garden:

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-ivy-terrace (192.168.10.45)
   Moss 0.2.1 • Up 5m

   OFFERINGS:
   └─ homeassistant    Running   Healthy   8123

No other stones found.
```

Only one Stone. Your other three Stones—on the main network (192.168.1.x)—are invisible. They're on a different subnet, and UDP broadcasts don't cross routers.

---

You need a bridge. Something that both networks can reach. You have a small VPS that's accessible from both VLANs. Time to light a Lantern.

On your VPS:

```bash
garden-rake place lantern
```

```
Installing Lantern registry...

  Downloading garden-lantern... done
  Creating systemd service... done
  Starting garden-lantern... done

✓ Lantern is now running at http://192.168.1.5:7186

Configure your Stones to register with this Lantern:
  export LANTERN_ENDPOINT=http://192.168.1.5:7186

Or add to /etc/zen-garden/moss.conf:
  lantern_endpoint = "http://192.168.1.5:7186"
```

The Lantern is lit. Now you need to tell your Stones where to find it.

---

On each Stone, you add the Lantern endpoint:

```bash
# On stone-amber-ridge
echo 'lantern_endpoint = "http://192.168.1.5:7186"' >> /etc/zen-garden/moss.conf
systemctl restart garden-moss
```

You repeat this on all four Stones. Then check the Lantern:

```bash
curl http://192.168.1.5:7186/api/v1/stones | jq
```

```json
{
  "stones": [
    {
      "name": "stone-amber-ridge",
      "endpoint": "http://192.168.1.42:7185",
      "status": "online",
      "services": ["mongodb", "redis"],
      "last_seen": "2026-01-30T10:45:12Z"
    },
    {
      "name": "stone-coral-reef",
      "endpoint": "http://192.168.1.58:7185",
      "status": "online",
      "services": ["postgres"],
      "last_seen": "2026-01-30T10:45:08Z"
    },
    {
      "name": "stone-bronze-canyon",
      "endpoint": "http://192.168.1.73:7185",
      "status": "online",
      "services": ["elasticsearch"],
      "last_seen": "2026-01-30T10:45:15Z"
    },
    {
      "name": "stone-ivy-terrace",
      "endpoint": "http://192.168.10.45:7185",
      "status": "online",
      "services": ["homeassistant"],
      "last_seen": "2026-01-30T10:45:11Z"
    }
  ],
  "last_updated": "2026-01-30T10:45:15Z"
}
```

All four Stones are registered. The Lantern sees across both subnets.

---

Now, back on stone-ivy-terrace (the IoT Stone):

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-ivy-terrace (192.168.10.45) [local]
   Moss 0.2.1 • Up 15m

   OFFERINGS:
   └─ homeassistant    Running   Healthy   8123

●  stone-amber-ridge (192.168.1.42) [via lantern]
   Moss 0.2.1 • Up 67d

   OFFERINGS:
   ├─ mongodb     Running   Healthy   27017
   └─ redis       Running   Healthy   6379

●  stone-coral-reef (192.168.1.58) [via lantern]
   Moss 0.2.1 • Up 45d

   OFFERINGS:
   └─ postgres    Running   Healthy   5432

●  stone-bronze-canyon (192.168.1.73) [via lantern]
   Moss 0.2.1 • Up 7d

   OFFERINGS:
   └─ elasticsearch    Running   Healthy   9200
```

Four Stones visible. The `[via lantern]` tag shows which ones were discovered through the registry rather than direct UDP.

---

Your Home Assistant on the IoT network needs to connect to the MongoDB on your main network. Previously impossible—different subnets. Now:

```bash
garden-rake find mongodb
```

```
Found 1 offering matching 'mongodb':

  mongodb on stone-amber-ridge (192.168.1.42:27017)
    Health: healthy
    Discovered via: lantern

Connection string: mongodb://192.168.1.42:27017
```

The IoT Stone can resolve services on the main network. As long as the firewall allows the actual connection (port 27017), your Home Assistant can reach MongoDB.

---

A few days later, your VPS needs a reboot for kernel updates. The Lantern goes offline:

```bash
garden-rake observe
```

```
Discovering garden...

⚠ Lantern unreachable (http://192.168.1.5:7186)
  Falling back to local discovery...

●  stone-ivy-terrace (192.168.10.45) [local]
   OFFERINGS:
   └─ homeassistant    Running   Healthy

No other stones found on local network.

Note: 3 stones may be available via Lantern when it returns.
```

The Lantern is down, but the garden degrades gracefully. Local discovery still works. Services on the same subnet are still found.

When the VPS comes back up, the Stones automatically re-register. The cross-subnet view returns.

---

## What Just Happened

### The Subnet Problem

UDP broadcasts don't cross routers. This is by design—you don't want every broadcast on your network hitting every device on the internet.

```
   192.168.1.0/24 (Main)          192.168.10.0/24 (IoT)
┌───────────────────────┐      ┌───────────────────────┐
│ stone-amber-ridge     │      │ stone-ivy-terrace     │
│ stone-coral-reef      │      │                       │
│ stone-bronze-canyon   │      │                       │
└──────────┬────────────┘      └──────────┬────────────┘
           │                              │
           │    ╔════════════════════╗    │
           └────║     ROUTER         ║────┘
                ║ (blocks UDP bcast) ║
                ╚════════════════════╝
```

Stones on the main network can chirp to each other. Stone-ivy-terrace chirps alone. Nobody answers.

### The Lantern Solution

Lantern is an HTTP registry that both subnets can reach:

```
   192.168.1.0/24 (Main)          192.168.10.0/24 (IoT)
┌───────────────────────┐      ┌───────────────────────┐
│ stone-amber-ridge ────┼──────┼──→ LANTERN ←─────────│─ stone-ivy-terrace
│ stone-coral-reef  ────┼──────┼──→ :7186   ←─────────│
│ stone-bronze-canyon ──┼──────┼──→         ←─────────│
└───────────────────────┘      └───────────────────────┘

                        Stones register via HTTP
                        Queries return full topology
```

Lantern doesn't route traffic—it just answers "where is everything?" Actual service connections go directly between Stones.

### Registration Protocol

Every 45 seconds, each Stone sends a heartbeat to Lantern:

```http
POST /api/v1/register
Content-Type: application/json

{
  "stone_id": "019c3a2b-4d5e-7f89-a1b2-c3d4e5f67890",
  "stone_name": "stone-amber-ridge",
  "endpoint": "http://192.168.1.42:7185",
  "services": [
    {
      "name": "mongodb",
      "service_type": "mongodb",
      "status": "running",
      "connection_string": "mongodb://192.168.1.42:27017"
    }
  ]
}
```

Lantern responds:

```json
{
  "ttl_seconds": 60,
  "next_heartbeat_seconds": 45
}
```

If a Stone misses its heartbeat for 60 seconds, Lantern marks it offline. The Stone remains in the registry (in case it comes back) but won't be returned in queries.

### Discovery Priority

When you run `garden-rake find mongodb`, the discovery cascade tries:

```
1. LOCAL CACHE (<1ms)
   └─ Recent discovery results still valid? Use them.

2. UDP BROADCAST (<100ms)
   └─ Send to 239.255.42.99:7184, collect responses
   └─ Works on same subnet only

3. mDNS BROWSE (<50ms)
   └─ Query _moss._tcp.local.
   └─ Works on same subnet only (usually)

4. LANTERN HTTP (<200ms)
   └─ GET http://lantern:7186/api/v1/resolve?service=mongodb
   └─ Works across subnets!

5. EXPLICIT --at FLAG
   └─ User override, skip all discovery
```

Lantern is the fallback when local discovery fails. This means:
- Same-subnet discovery is still fast (UDP)
- Cross-subnet discovery works (HTTP to Lantern)
- No Lantern required for simple single-subnet setups

### Graceful Degradation

Lantern is optional infrastructure. If it goes down:

- Stones on the same subnet still discover each other (UDP)
- Cross-subnet discovery fails gracefully
- Services continue running (Lantern doesn't proxy traffic)
- When Lantern returns, Stones automatically re-register

You lose visibility, not functionality. The garden keeps working; you just can't see across subnets until Lantern recovers.

### Security Considerations

Lantern registrations use bearer token authentication:

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

The token includes:
- Stone name (who's registering)
- Operation (register, resolve)
- Timestamp and nonce (prevent replay)
- Signature (HMAC-SHA256)

This prevents rogue devices from registering fake Stones. However, Lantern is designed for trusted internal networks—it's not hardened for internet exposure.

---

## When You Need a Lantern

**You probably don't need a Lantern if:**
- All Stones are on the same subnet
- You have 3-5 Stones in a simple home lab
- mDNS works reliably on your network

**You should consider a Lantern if:**
- Stones span multiple VLANs or subnets
- You have Windows clients (mDNS can be unreliable)
- You want centralized visibility of all Stones
- Docker Desktop isolates your network
- Your network blocks multicast

A Lantern is infrastructure. It needs to be reachable from all subnets, which often means a dedicated machine or VM. Don't add complexity unless you need cross-subnet discovery.

---

## Commands From This Journey

```bash
# Install Lantern on current machine
garden-rake place lantern

# Install Lantern on remote Stone
garden-rake place lantern at stone-bronze-canyon

# List all known Lanterns
garden-rake show lanterns

# Configure a Stone to use Lantern
echo 'lantern_endpoint = "http://192.168.1.5:7186"' >> /etc/zen-garden/moss.conf

# Query Lantern directly
curl http://192.168.1.5:7186/api/v1/stones

# Resolve service via Lantern
curl 'http://192.168.1.5:7186/api/v1/resolve?service=mongodb'

# Check Lantern health
curl http://192.168.1.5:7186/health

# Force discovery via Lantern (skip UDP)
garden-rake find mongodb --via-lantern
```

---

*Zen Garden Documentation — Journeys*
