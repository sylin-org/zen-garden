# The Stray Container

*There's a MongoDB running on this machine. The garden doesn't know about it.*

---

## The Story

You're setting up a new Stone—an old workstation you've repurposed. Before installing Zen Garden, you were running some Docker containers manually. A MongoDB for a side project. A Redis for caching. Maybe a Postgres you forgot about.

After installing Moss, you check your garden:

```bash
garden-rake observe
```

```
●  stone-silver-creek (192.168.1.89)
   Moss 0.2.1 • Up 10m

   OFFERINGS:
   (none)
```

The garden shows no offerings. But you know there are containers running:

```bash
docker ps
```

```
CONTAINER ID   IMAGE         PORTS                     NAMES
a1b2c3d4e5f6   mongo:7       0.0.0.0:27017->27017/tcp  my-mongo
f6e5d4c3b2a1   redis:7       0.0.0.0:6379->6379/tcp    project-redis
```

Two containers running, invisible to the garden. Strays.

---

You ask the garden to look for strays:

```bash
garden-rake locate-strays
```

```
Scanning for adoptable containers...

Found 2 containers not managed by Zen Garden:

  my-mongo (mongo:7)
    Port: 27017
    Status: running
    Detected as: mongodb
    ✓ Can be adopted

  project-redis (redis:7)
    Port: 6379
    Status: running
    Detected as: redis
    ✓ Can be adopted

Use 'garden-rake adopt <name>' to bring a container into the garden.
```

The garden found them. It detected what they are based on the image name and exposed ports. Now you can adopt them.

---

```bash
garden-rake adopt my-mongo
```

```
Adopting container 'my-mongo'...

  Detected offering: mongodb
  Container: my-mongo → zen-offering-mongodb
  Port: 27017
  Health check: passed

✓ mongodb adopted

  The container has been renamed to follow garden conventions.
  Your data and configuration are preserved.
  The service is now discoverable as 'mongodb' in the garden.
```

The container was renamed from `my-mongo` to `zen-offering-mongodb`. This is how the garden tracks its offerings—by naming convention. Your MongoDB data, volumes, and configuration are untouched.

```bash
garden-rake observe
```

```
●  stone-silver-creek (192.168.1.89)
   Moss 0.2.1 • Up 12m

   OFFERINGS:
   └─ mongodb    Running   Healthy   27017
```

The garden now knows about MongoDB. Other Stones can discover it. Your applications can find it via `zen-garden:mongodb`.

---

You adopt the Redis too:

```bash
garden-rake adopt project-redis
```

```
Adopting container 'project-redis'...

  Detected offering: redis
  Container: project-redis → zen-offering-redis
  Port: 6379
  Health check: passed

✓ redis adopted
```

Now both services are in the garden. But you notice something: the MongoDB has a custom configuration you set up months ago. Will the garden preserve it?

```bash
garden-rake status mongodb on stone-silver-creek
```

```
mongodb on stone-silver-creek

  Status: Running
  Health: Healthy
  Mode: Adopted
  Image: mongo:7.0.5
  Port: 27017

  Control: Full
    ✓ Can start/stop/restart
    ✓ Health monitoring enabled
    ✓ Auto-restart on failure

  Volumes:
    /var/lib/mongo-data → /data/db

  Note: This offering was adopted from existing container.
        Original config preserved. Use 'garden-rake nourish' to update image.
```

The `Mode: Adopted` shows this wasn't deployed fresh—it was brought in from outside. Your volumes, environment variables, everything stays the same. The garden just wraps around it.

---

A few weeks later, you want to update the adopted MongoDB:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 1 available, 0 blocked

───────────────────────────────────────────────────────────────

  stone-silver-creek
    AVAILABLE:
      • mongodb 7.0.5 → 7.0.8 (adopted)
        Note: Adopted container will be migrated to garden-managed

───────────────────────────────────────────────────────────────
```

The update is available. But notice the note: updating an adopted container migrates it to garden-managed. This means:

- The garden will create a new container with the updated image
- Your data will be preserved (volumes are kept)
- The custom configuration might need review

You apply the update:

```
Applying update...

  [1/1] Nourishing mongodb on stone-silver-creek
        Collecting harvest... done
        Creating new container from mongo:7.0.8... done
        Migrating volumes... done
        Starting container... done
        Verifying health... passed
        Removing old container... done
        ✓ mongodb 7.0.5 → 7.0.8

mongodb is now garden-managed (no longer adopted mode).
```

The MongoDB has graduated from "adopted stray" to "garden-managed offering." Future updates will be seamless.

---

## What Just Happened

### Container Namespace

The garden uses a naming convention to identify its containers:

```
zen-offering-{name}      Managed offerings
zen-companion-{x}-{y}    Sidecars and helpers
```

Containers without these prefixes are invisible to the garden. This is intentional—we don't want the garden accidentally managing your unrelated containers.

When you `docker ps`, you might see:

```
zen-offering-mongodb     ← Garden knows about this
zen-offering-redis       ← Garden knows about this
my-postgres              ← Invisible to garden
traefik                  ← Invisible to garden
```

### Adoption Detection

When you run `locate-strays`, the garden:

1. **Lists all Docker containers** on the Stone
2. **Filters out** any with `zen-` prefix (already managed)
3. **Analyzes each container** to detect what it is:
   - Image name matching (`mongo:*` → mongodb)
   - Port matching (27017 → mongodb, 6379 → redis)
   - HTTP probes (check for known endpoints)
4. **Reports** adoptable containers with detected offerings

The detection uses manifest rules—the same templates that define official offerings.

### What Adoption Does

```
Before adoption:

  Container: my-mongo
  ├─ Image: mongo:7
  ├─ Volumes: /var/lib/mongo-data:/data/db
  ├─ Ports: 27017:27017
  └─ Environment: MONGO_INITDB_ROOT_USERNAME=admin

After adoption:

  Container: zen-offering-mongodb  (renamed)
  ├─ Image: mongo:7                (unchanged)
  ├─ Volumes: /var/lib/mongo-data:/data/db  (unchanged)
  ├─ Ports: 27017:27017            (unchanged)
  ├─ Environment: (unchanged)
  └─ Labels: zen.garden.offering=mongodb  (added)
```

Adoption is a rename plus metadata. Your container keeps running. No restart. No data migration. Just a new name that the garden recognizes.

### Control Levels

Adopted containers have a control level:

| Level | What Garden Can Do |
|-------|-------------------|
| `full` | Start, stop, restart, health check, auto-restart |
| `monitor` | Health monitoring only, no lifecycle control |
| `announce` | Just make discoverable, no monitoring |

Default is `full`. You can specify when adopting:

```bash
garden-rake adopt my-mongo --control monitor
```

Use `monitor` for containers you want visible but don't want the garden touching. Use `announce` for external services you just want discoverable.

### Borrowed Services

What about services that aren't containers at all? You have a Postgres cluster managed by your DBA, running on dedicated hardware. You want it discoverable but not managed.

```bash
garden-rake borrow postgres from postgres://db-server.local:5432
```

```
Registering external service...

  Name: postgres
  URL: postgres://db-server.local:5432
  Control: Announce only

✓ postgres is now discoverable as 'zen-garden:postgresql'
```

Borrowed services aren't containers. They're just entries in the registry that point to external endpoints. The garden doesn't manage them—it just knows they exist.

```bash
garden-rake find postgresql
```

```
Found 1 offering matching 'postgresql':

  postgres on stone-silver-creek (db-server.local:5432)
    Health: (external, not monitored)
    Type: Borrowed

Connection string: postgresql://db-server.local:5432
```

To stop tracking a borrowed service:

```bash
garden-rake return-borrowed postgres
```

### Auto-Adoption

You don't have to manually adopt every container. The garden can automatically adopt matching containers:

```bash
# In moss.conf
auto_adopt = true
auto_adopt_exclude = ["traefik", "portainer"]
```

With auto-adoption enabled:
- Moss scans for new containers every 5 minutes
- Containers matching known offerings are adopted automatically
- Excluded names/patterns are skipped
- Adopted containers appear in the garden within minutes

This is useful when migrating an existing Docker setup to Zen Garden—just enable auto-adopt and watch your containers join the garden one by one.

---

## Migration Path

The typical journey from "random Docker containers" to "managed garden":

```
Phase 1: Discovery
  └─ Install Moss
  └─ Run locate-strays
  └─ Review what's running

Phase 2: Adoption
  └─ Adopt containers you want managed
  └─ Borrow external services
  └─ Set appropriate control levels

Phase 3: Operation
  └─ Services are now discoverable
  └─ Health monitoring active
  └─ Can use garden-rake for management

Phase 4: Migration (optional)
  └─ Run nourish to update images
  └─ Adopted containers become garden-managed
  └─ Full control with garden templates
```

You don't have to migrate all at once. Adoption is non-destructive. Take your time.

---

## Commands From This Journey

```bash
# Find containers not managed by garden
garden-rake locate-strays

# Adopt a container into the garden
garden-rake adopt my-mongo

# Adopt with specific control level
garden-rake adopt my-mongo --control monitor

# Register external service (not a container)
garden-rake borrow postgres from postgres://db-server.local:5432

# Stop tracking borrowed service
garden-rake return-borrowed postgres

# Release adopted container (keep running, stop managing)
garden-rake release mongodb

# Check adoption status
garden-rake status mongodb on stone-silver-creek

# Enable auto-adoption in config
# auto_adopt = true
```

---

*Zen Garden Documentation — Journeys*
