# Zen Garden

<p align="center">
  <img src="res/zg-256.png" alt="Zen Garden" />
</p>

Service discovery and orchestration for self-hosted infrastructure on repurposed hardware.

---

## The Situation

Every year, humanity generates 62 million tonnes of electronic waste.

Much of it works. Laptops discarded because they can't run Windows 11. Servers decommissioned because they're "out of support." Thin clients abandoned because a vendor stopped making drivers. The machines function. They simply don't function for what the _market_ wants.

Meanwhile, self-hosting remains hard — not because the software is complex, but because **machines fail**. When your database laptop dies, you face a choice: rename the replacement to match the old hostname, or update every application's configuration. Most people give up and pay for managed databases.

Zen Garden exists because both of these problems have the same solution.

---

## How It Works

```bash
# Traditional: tightly coupled to machines
MONGODB_URI=mongodb://old-laptop-01.local:27017

# Zen Garden: coupled to services
MONGODB_URI=zen-garden:mongodb/mydb
```

Stones (devices running Moss) provide service discovery, orchestration, and failure recovery using mDNS multicast, Docker, and curated offering templates. Security and multi-subnet scaling are opt-in.

| Layer            | What happens                                                       |
| ---------------- | ------------------------------------------------------------------ |
| Discovery        | mDNS multicast (same protocol as AirPlay, Chromecast)              |
| Orchestration    | Docker containers from curated offering templates                  |
| Failure recovery | Clients retry mDNS — new Stone, same service type, auto-reconnect |
| Security         | Optional mTLS via Pond (explicit trust boundary)                   |
| Scaling          | mDNS for up to 30 Stones; Lantern registry recommended beyond 20  |

- **Set up real hardware** — [First Stone Guide](docs/guides/first-stone.md)
- **Understand the protocol** — [Discovery Spec](docs/specs/discovery.md)

---

## Vocabulary

| Term        | What It Is                                                              |
| ----------- | ----------------------------------------------------------------------- |
| **Stone**   | A device offering services (laptop, desktop, Raspberry Pi, thin client) |
| **Moss**    | The daemon running on each Stone (port 7185)                            |
| **Rake**    | The CLI tool for managing the garden                                    |
| **Lantern** | Optional registry for multi-subnet gardens                              |
| **Pond**    | Optional security boundary (mTLS)                                       |

---

## Getting Started

```bash
# Start a Stone with MongoDB
docker run -d -p 27017:27017 --name mongo-stone \
  -e ANNOUNCE_SERVICE=mongodb \
  zen-garden/stone:latest

# Discover what's running
garden-rake find mongodb
# => Found: mongodb on stone-01 (192.168.1.42:27017)
```

---

## Examples

A typical garden runs on whatever hardware you have.

| Hardware                  | Service    | Notes                  |
| ------------------------- | ---------- | ---------------------- |
| 2015 laptop               | MongoDB    | Handles 1,000+ req/sec |
| Dell Wyse thin client     | Redis      | 2 GB RAM, 5 watts      |
| Old desktop               | MinIO      | File storage           |
| Raspberry Pi              | Prometheus | Monitoring             |

---

## Documentation

| If you want to...               | Start here                                                            |
| ------------------------------- | --------------------------------------------------------------------- |
| Set up your first Stone         | [First Stone Guide](docs/guides/first-stone.md)                       |
| Understand the philosophy       | [Humanist Infrastructure](docs/philosophy/humanist-infrastructure.md) |
| See what services are available | [Offerings Catalog](docs/reference/offerings.md)                      |
| Learn the CLI                   | [Rake Commands](docs/specs/rake-commands.md)                          |
| Understand security options     | [Security Overview](docs/security/overview.md)                        |
| Read the specifications         | [Moss Daemon Lifecycle](docs/specs/moss-daemon-lifecycle.md)          |
| See architecture decisions      | [Decision Records](docs/decisions/)                                   |

[Documentation Hub](docs/README.md)

---

## Project Status

Version 0.1.0 — active development.

| Component        | Description                         |
| ---------------- | ----------------------------------- |
| `garden-moss`    | Daemon with 14-phase startup        |
| `garden-rake`    | CLI with full command taxonomy      |
| `garden-lantern` | Registry for cross-subnet discovery |
| Offerings        | 30+ curated service templates       |

- [Release Notes](docs/ops/release-notes.md)

---

## Contributing

- Run Stones on your old hardware and tell us what breaks
- Write offering templates for services you use
- Improve documentation where it confused you
- [Maintainer Docs](docs/ops/maintainers.md)

---

**License**: Apache 2.0
**Maintainer**: Sylin.org
**Repository**: [github.com/sylin-org/zen-garden](https://github.com/sylin-org/zen-garden)
