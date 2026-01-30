# Borrowing from Outside

*Not everything in your garden runs on a Stone.*

---

## The Story

Your company has a production Postgres cluster. Three nodes, managed by the DBA team, running on dedicated hardware. It's not part of your Zen Garden—and it shouldn't be. But your applications need to find it.

You could hardcode the connection string. But then it's not discoverable like your other services. Your developers have to know about it separately. It's outside the garden's view.

There's another way: borrow it.

```bash
garden-rake borrow prod-postgres from postgresql://db-primary.corp.local:5432
```

```
Registering external service...

  Name: prod-postgres
  URL: postgresql://db-primary.corp.local:5432
  Protocol: postgresql
  Host: db-primary.corp.local
  Port: 5432

✓ prod-postgres is now discoverable in the garden

  Connection string: zen-garden:prod-postgres
  Note: This is a borrowed service. The garden announces it but doesn't manage it.
```

The Postgres cluster isn't running on a Stone. It's not a container. The garden doesn't manage it, can't restart it, can't update it. But now it's visible.

---

```bash
garden-rake find postgresql
```

```
Found 2 offerings matching 'postgresql':

  postgres on stone-coral-reef (192.168.1.58:5432)
    Health: healthy
    Type: Managed

  prod-postgres on stone-amber-ridge (db-primary.corp.local:5432)
    Health: (external)
    Type: Borrowed

Which would you like?
  [1] postgres (managed, local dev)
  [2] prod-postgres (borrowed, production)
```

Two Postgres services. One managed by the garden (your local dev instance). One borrowed (the production cluster). Both discoverable through the same mechanism.

Your application can use `zen-garden:prod-postgres` just like any other service:

```python
# Application code
db_uri = resolve("zen-garden:prod-postgres")
# Returns: postgresql://db-primary.corp.local:5432
```

---

You have more external services. A NAS with SMB shares. A network printer. An MQTT broker running on dedicated hardware.

```bash
garden-rake borrow file-server from smb://nas.local/shared
garden-rake borrow office-printer from ipp://printer.local:631
garden-rake borrow mqtt-broker from mqtt://mqtt.corp.local:1883
```

```
✓ file-server registered (smb://nas.local/shared)
✓ office-printer registered (ipp://printer.local:631)
✓ mqtt-broker registered (mqtt://mqtt.corp.local:1883)
```

Now you have a unified view:

```bash
garden-rake borrowed
```

```
Borrowed services on stone-amber-ridge:

  NAME             PROTOCOL    HOST                    PORT
  prod-postgres    postgresql  db-primary.corp.local   5432
  file-server      smb         nas.local               (share: /shared)
  office-printer   ipp         printer.local           631
  mqtt-broker      mqtt        mqtt.corp.local         1883

4 borrowed services registered.
Use 'garden-rake return <name>' to unregister.
```

These aren't containers. They're not managed. But they're visible. Your developers can discover them. Your monitoring can track them. Your documentation knows they exist.

---

Six months later, the DBA team migrates the Postgres cluster to new hardware. The hostname changes.

```bash
garden-rake return prod-postgres
garden-rake borrow prod-postgres from postgresql://db-cluster.corp.local:5432
```

```
✓ prod-postgres unregistered
✓ prod-postgres registered at new location
```

Your applications using `zen-garden:prod-postgres` automatically get the new address on their next connection. No config file changes. No deployments. Just update the borrowed service registration.

---

## What Just Happened

### The Shakkei Concept

In Japanese garden design, there's a technique called **shakkei** (借景)—"borrowed scenery." A garden might frame a distant mountain as part of its composition, even though the mountain isn't part of the garden itself.

Borrowed services work the same way. The production Postgres cluster isn't part of your garden. But you borrow its presence—you make it visible within your garden's frame of reference.

### What Gets Stored

When you borrow a service, the garden stores:

```rust
BorrowedOfferingInfo {
    name: "prod-postgres",           // Your chosen name
    offering: "prod-postgres",       // Same as name for borrowed
    mode: OfferingMode::Borrowed,    // Marks this as external
    location: ServiceLocation {
        host: "db-primary.corp.local",
        port: 5432,
        protocol: "postgresql",
    },
    announced_at: "2026-01-30T15:30:00Z",
    connection_template: "postgresql://db-primary.corp.local:5432",
}
```

That's it. No container ID. No volume mounts. No restart policy. Just enough to announce and resolve.

### Control Levels

Borrowed services have minimal control:

| Level | Managed | Adopted | Borrowed |
|-------|---------|---------|----------|
| Start/stop | ✓ | ✓ | ✗ |
| Restart | ✓ | ✓ | ✗ |
| Health monitoring | ✓ | ✓ | Optional |
| Updates | ✓ | ✓ | ✗ |
| Discovery | ✓ | ✓ | ✓ |

The garden can announce borrowed services but can't manage their lifecycle. They're read-only from the garden's perspective.

### Health Checking (Optional)

By default, borrowed services show `(external)` health status—the garden doesn't probe them. But you can enable basic health checks:

```bash
garden-rake borrow api-gateway from http://gateway.corp.local:8080 --health-check tcp
```

With `--health-check tcp`, the garden periodically opens a TCP connection to verify the service is reachable. With `--health-check http`, it makes an HTTP request to the URL.

This is passive monitoring—the garden reports what it sees but doesn't try to fix problems.

### Discovery Resolution

When an application resolves `zen-garden:prod-postgres`:

```
1. Query local Stone for service "prod-postgres"
2. Find BorrowedOfferingInfo in registry
3. Return connection_template: "postgresql://db-primary.corp.local:5432"
4. Application connects directly to external service
```

The garden doesn't proxy the connection. It just provides the address. Traffic flows directly from your application to the external service.

### Why Not Just Use DNS?

You could put external services in DNS. But:

- DNS doesn't know service types (is `db.corp.local` Postgres or MySQL?)
- DNS doesn't integrate with your garden's discovery
- DNS updates require IT coordination
- DNS doesn't provide a unified service catalog

Borrowed services give you a single interface—`garden-rake find`—for discovering everything, whether it's a container on a Stone or a mainframe in the data center.

---

## The Three Modes

Your garden now has three types of services:

**Managed** — Full lifecycle control
```
garden-rake offer postgres
```
The garden deploys, monitors, updates, and can restart these services.

**Adopted** — Existing containers brought under management
```
garden-rake adopt my-mongo
```
Pre-existing Docker containers, now tracked and controlled by the garden.

**Borrowed** — External services made visible
```
garden-rake borrow prod-db from postgresql://...
```
Not controlled, just announced. The mountain in the distance.

All three appear in discovery. All three can be found with `garden-rake find`. The difference is in control, not visibility.

---

## Commands From This Journey

```bash
# Register an external service
garden-rake borrow prod-postgres from postgresql://db.corp.local:5432

# Register with optional health checking
garden-rake borrow api-gateway from http://gateway.local:8080 --health-check http
garden-rake borrow cache-cluster from redis://cache.local:6379 --health-check tcp

# List all borrowed services
garden-rake borrowed

# Unregister a borrowed service
garden-rake return prod-postgres

# Find services (includes borrowed)
garden-rake find postgresql

# Update borrowed service location
garden-rake return old-service
garden-rake borrow old-service from new-url
```

---

*Zen Garden Documentation — Journeys*
