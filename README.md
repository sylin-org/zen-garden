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

Your app asks "Where's MongoDB?" — a Stone answers, connection established. When hardware fails, swap in a replacement. The new Stone announces the same service and apps reconnect automatically.

**Discovery** — Stones announce services via mDNS multicast (same protocol as AirPlay and Chromecast). No central registry required for small gardens.

**Orchestration** — Each Stone runs Docker. Services are defined as "offerings" — curated templates with sensible defaults selected from a catalog.

**Failure handling** — When a Stone disappears, apps retry discovery. When a new Stone offers the same service, apps reconnect. Hardware becomes interchangeable.

**Security** (optional) — Fill a Pond to add mTLS. Stones inside the pond trust each other.

**Scale** — 3–10 Stones work with mDNS alone. Beyond that, add a Lantern (registry service).

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

**Set up real hardware** — [First Stone Guide](docs/guides/first-stone.md)
**Understand the protocol** — [Discovery Spec](docs/specs/discovery.md)

---

## Examples

A typical garden: 3–10 Stones running on whatever hardware you have.

- A 2015 laptop running MongoDB (handles 1,000+ req/sec)
- A Dell Wyse thin client running Redis (2GB RAM, 5 watts, silent)
- An old desktop serving files via MinIO
- A Raspberry Pi running Prometheus

All discovered automatically via mDNS. All managed through `garden-rake`.

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

**Version**: 0.1.0 — Active development

- `garden-moss` daemon with 14-phase startup orchestration
- `garden-rake` CLI with full command taxonomy
- `garden-lantern` registry for cross-subnet discovery
- 30+ service offering templates

See [Release Notes](docs/ops/release-notes.md) for details.

---

## Contributing

- Run Stones on your old hardware and tell us what breaks
- Write offering templates for services you use
- Improve documentation where it confused you

See [Maintainer Docs](docs/ops/maintainers.md) for architecture invariants.

---

**License**: Apache 2.0
**Maintainer**: Sylin.org
**Repository**: [github.com/sylin-org/zen-garden](https://github.com/sylin-org/zen-garden)
